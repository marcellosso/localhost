//! The shared core on this machine: one Service manager that the app,
//! `localhost mcp` and the `localhost` command all drive, so a Service one
//! of them starts is Running in the others, and a stop from any of them
//! stops the same Run.
//!
//! It is served on a Unix socket in the storage root, `localhostd.sock`,
//! speaking this module's protocol, by whichever localhost executable the
//! first client started: `localhost daemon`, or the app's own with
//! `--daemon`. One per storage root, held by a lock file; only this user
//! may connect. It writes nothing else but its Runs' logs and records,
//! where the in-process core always kept them.
//!
//! Its lifetime follows its clients. A client disconnecting stops the Runs
//! nobody needs any more (`Client`); once none has been connected for
//! `IDLE_TIMEOUT`, it exits. SIGTERM, SIGINT and SIGHUP stop every Run
//! first.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Notify;

use super::server::Core;
use super::{Client, Remote};
use crate::{Link, LocalhostError, ServiceManagerOptions};

const SOCKET_NAME: &str = "localhostd.sock";
const LOCK_NAME: &str = "localhostd.lock";
/// Where a started core's standard error goes: it has no terminal.
const LOG_NAME: &str = "localhostd.log";

/// How long the core waits, with no client connected, before it exits:
/// long enough for the app to relaunch, or an agent to reconnect.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a client waits for a core it started to accept connections.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// The variables a started core keeps from its client's environment. A
/// Run's environment starts from the core's, then the login shell's
/// profile; a client's own variables (an editor's, a terminal's) mustn't
/// leak into every later Run, whoever starts it.
const KEPT_VARIABLES: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PATH",
    "TMPDIR",
    "LANG",
    "LANGUAGE",
    "TZ",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "SSH_AUTH_SOCK",
    "__CF_USER_TEXT_ENCODING",
];

/// The longest socket path every Unix takes (`sun_path` is 104 bytes on
/// macOS, with its terminating NUL).
const MAX_SOCKET_PATH: usize = 103;

/// `localhostd.sock` in the storage root, or, if that path is too long for
/// a socket, a name derived from it in the temporary folder.
pub fn socket_path(storage_root: &Path) -> PathBuf {
    let path = storage_root.join(SOCKET_NAME);
    if path.as_os_str().len() <= MAX_SOCKET_PATH {
        return path;
    }
    // FNV-1a: stable across versions and processes, unlike `DefaultHasher`.
    let hash = storage_root
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
    std::env::temp_dir().join(format!("localhostd-{hash:016x}.sock"))
}

fn unavailable(reason: impl Into<String>) -> LocalhostError {
    LocalhostError::CoreUnavailable(reason.into())
}

// MARK: Clients

/// How a client reaches this machine's Services: through the shared core of
/// `storage_root`, started with `daemon` if none is running.
pub fn link(storage_root: &Path, client: Client, daemon: std::process::Command) -> Link {
    let storage_root = storage_root.to_path_buf();
    Link::Remote(Box::pin(async move { connect(&storage_root, client, Some(daemon)).await }))
}

/// Connects to the shared core of `storage_root` as `client`. If none
/// answers, starts one with `daemon` (e.g. `localhost daemon <root>`), in a
/// session of its own so it outlives this process, and waits for it.
pub async fn connect(
    storage_root: &Path,
    client: Client,
    daemon: Option<std::process::Command>,
) -> Result<Remote, LocalhostError> {
    let path = socket_path(storage_root);
    let stream = match UnixStream::connect(&path).await {
        Ok(stream) => stream,
        Err(error) => {
            let Some(daemon) = daemon else { return Err(unavailable(error.to_string())) };
            start(daemon, storage_root)?;
            wait_for(&path).await?
        }
    };
    let (reader, writer) = stream.into_split();
    let remote = Remote::connect_owning(reader, writer, None, LocalhostError::CoreUnavailable).await?;
    remote.identify(client).await?;
    Ok(remote)
}

fn start(mut daemon: std::process::Command, storage_root: &Path) -> Result<(), LocalhostError> {
    std::fs::create_dir_all(storage_root).map_err(|error| unavailable(error.to_string()))?;
    let log = OpenOptions::new().create(true).append(true).open(storage_root.join(LOG_NAME));
    daemon
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log.map_or_else(|_| Stdio::null(), Stdio::from))
        .current_dir("/")
        .env_clear()
        .envs(std::env::vars_os().filter(|(name, _)| {
            let name = name.to_string_lossy();
            KEPT_VARIABLES.contains(&name.as_ref()) || name.starts_with("LC_") || name.starts_with("XDG_")
        }));
    // SAFETY: `setsid` is async-signal-safe, and nothing else runs between
    // fork and exec.
    unsafe {
        daemon.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = daemon.spawn().map_err(|error| unavailable(format!("it couldn't be started: {error}")))?;
    // Reaped once it exits, if this process is still there to.
    std::thread::spawn(move || child.wait());
    Ok(())
}

async fn wait_for(path: &Path) -> Result<UnixStream, LocalhostError> {
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
    loop {
        match UnixStream::connect(path).await {
            Ok(stream) => return Ok(stream),
            Err(_) if tokio::time::Instant::now() < deadline => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(error) => return Err(unavailable(format!("it didn't start in time ({error})"))),
        }
    }
}

// MARK: The core

/// Serves the shared core of `storage_root` until it is idle or told to
/// end, on a runtime of its own: what `localhost daemon` and the app's
/// `--daemon` run. Returns at once if another process already serves it.
pub fn run(storage_root: &Path) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Couldn't start: {error}"))?;
    let Some(_lock) =
        lock(storage_root).map_err(|error| format!("Couldn't lock {}: {error}", storage_root.display()))?
    else {
        return Ok(());
    };
    runtime.block_on(async {
        let listener = bind(storage_root).map_err(|error| format!("Couldn't listen: {error}"))?;
        let mut terminate = signal(SignalKind::terminate()).map_err(|error| error.to_string())?;
        let mut interrupt = signal(SignalKind::interrupt()).map_err(|error| error.to_string())?;
        let mut hang_up = signal(SignalKind::hangup()).map_err(|error| error.to_string())?;
        let shutdown = async {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = interrupt.recv() => {}
                _ = hang_up.recv() => {}
            }
        };
        serve(listener, storage_root, ServiceManagerOptions::default(), IDLE_TIMEOUT, shutdown).await;
        Ok::<(), String>(())
    })?;
    // Still under the lock: no newer core has taken its place.
    let _ = std::fs::remove_file(socket_path(storage_root));
    Ok(())
}

/// The lock that makes this the only core of `storage_root`, held until
/// the returned file closes; `None` if another process holds it.
pub fn lock(storage_root: &Path) -> io::Result<Option<File>> {
    std::fs::create_dir_all(storage_root)?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(storage_root.join(LOCK_NAME))?;
    // SAFETY: a plain call on a descriptor this function owns.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(error) }
}

/// Listens on the socket of `storage_root`, replacing one a core that
/// ended unexpectedly left. Call it holding the lock.
pub fn bind(storage_root: &Path) -> io::Result<UnixListener> {
    let path = socket_path(storage_root);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Serves `listener`'s connections, all driving one Service manager whose
/// Runs keep their logs and records in `storage_root`, until no client has
/// been connected for `idle_timeout` or `shutdown` completes. Then stops
/// every Run.
pub async fn serve(
    listener: UnixListener,
    storage_root: &Path,
    options: ServiceManagerOptions,
    idle_timeout: Duration,
    shutdown: impl Future<Output = ()>,
) {
    let core = Core::new(storage_root, options);
    let changed = Arc::new(Notify::new());
    tokio::pin!(shutdown);
    loop {
        let is_idle = core.connection_count() == 0;
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) if is_this_user(&stream) => {
                    let core = core.clone();
                    let changed = changed.clone();
                    tokio::spawn(async move {
                        let (reader, writer) = stream.into_split();
                        core.serve_connection(reader, writer).await;
                        changed.notify_one();
                    });
                }
                Ok(_) => {}
                // Out of descriptors, say: try again shortly.
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            },
            _ = changed.notified() => {}
            // A connection accepted just before may not be counted yet.
            _ = tokio::time::sleep(idle_timeout), if is_idle => {
                if core.connection_count() == 0 {
                    break;
                }
            }
            _ = &mut shutdown => break,
        }
    }
    core.stop_every_run().await;
}

/// Whether the other end runs as this user: a client can run any command
/// as the user it reaches the core of.
fn is_this_user(stream: &UnixStream) -> bool {
    // SAFETY: `geteuid` can't fail.
    stream.peer_cred().is_ok_and(|credentials| credentials.uid() == unsafe { libc::geteuid() })
}
