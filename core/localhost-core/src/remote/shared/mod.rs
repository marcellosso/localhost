//! The shared core on this machine: one Service manager that the app,
//! `localhost mcp` and the `localhost` command all drive, so a Service one
//! of them starts is Running in the others, and a stop from any of them
//! stops the same Run.
//!
//! It is served on a local socket speaking this module's protocol, by
//! whichever localhost executable the first client started: `localhost
//! daemon`, or the app's own with `--daemon`. On macOS and Linux the socket
//! is a Unix socket in the storage root, `localhostd.sock` (`unix`); on
//! Windows a named pipe named after the storage root (`windows`). Only the
//! transport differs. One core per storage root, held by a lock file; only
//! this user may connect. It writes nothing else but its Runs' logs and
//! records, where the in-process core always kept them.
//!
//! Its lifetime follows its clients. A client disconnecting stops the Runs
//! nobody needs any more (`Client`); once none has been connected for
//! `IDLE_TIMEOUT`, it exits. Being told to end (SIGTERM, SIGINT, SIGHUP; on
//! Windows its console closing, logoff or shutdown) stops every Run first.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use super::server::Core;
use super::{Client, Remote};
use crate::{Link, LocalhostError, ServiceManagerOptions};

#[cfg(unix)]
use self::unix as transport;
#[cfg(unix)]
pub use self::unix::{Listener, bind, socket_path};
#[cfg(windows)]
use self::windows as transport;
#[cfg(windows)]
pub use self::windows::{Listener, bind, pipe_name};

const LOCK_NAME: &str = "localhostd.lock";
/// Where a started core's standard error goes: it has no terminal.
const LOG_NAME: &str = "localhostd.log";

/// How long the core waits, with no client connected, before it exits:
/// long enough for the app to relaunch, or an agent to reconnect.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a client waits for a core it started to accept connections.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// A name derived from the storage root, for a socket or pipe that can't
/// be in it. FNV-1a: stable across versions and processes, unlike
/// `DefaultHasher`.
fn hash(storage_root: &Path) -> u64 {
    storage_root
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
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
/// answers, starts one with `daemon` (e.g. `localhost daemon <root>`), on
/// its own so it outlives this process, and waits for it.
pub async fn connect(
    storage_root: &Path,
    client: Client,
    daemon: Option<std::process::Command>,
) -> Result<Remote, LocalhostError> {
    let (reader, writer) = match transport::connect(storage_root).await {
        Ok(stream) => stream,
        Err(error) => {
            let Some(daemon) = daemon else { return Err(unavailable(error.to_string())) };
            start(daemon, storage_root)?;
            wait_for(storage_root).await?
        }
    };
    let remote = Remote::connect_owning(reader, writer, None, LocalhostError::CoreUnavailable).await?;
    remote.identify(client).await?;
    Ok(remote)
}

fn start(mut daemon: std::process::Command, storage_root: &Path) -> Result<(), LocalhostError> {
    std::fs::create_dir_all(storage_root).map_err(|error| unavailable(error.to_string()))?;
    let log = OpenOptions::new().create(true).append(true).open(storage_root.join(LOG_NAME));
    daemon
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log.map_or_else(|_| std::process::Stdio::null(), std::process::Stdio::from));
    let mut child = transport::spawn_detached(daemon, storage_root)
        .map_err(|error| unavailable(format!("it couldn't be started: {error}")))?;
    // Reaped once it exits, if this process is still there to.
    std::thread::spawn(move || child.wait());
    Ok(())
}

async fn wait_for(storage_root: &Path) -> Result<(transport::Reader, transport::Writer), LocalhostError> {
    let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
    loop {
        match transport::connect(storage_root).await {
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
        let shutdown = transport::told_to_end().map_err(|error| error.to_string())?;
        serve(listener, storage_root, ServiceManagerOptions::default(), IDLE_TIMEOUT, shutdown).await;
        Ok::<(), String>(())
    })?;
    // Still under the lock: no newer core has taken its place.
    transport::clean_up(storage_root);
    Ok(())
}

/// The lock that makes this the only core of `storage_root`, held until
/// the returned file closes; `None` if another process holds it.
pub fn lock(storage_root: &Path) -> io::Result<Option<File>> {
    std::fs::create_dir_all(storage_root)?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(storage_root.join(LOCK_NAME))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// Serves `listener`'s connections, all driving one Service manager whose
/// Runs keep their logs and records in `storage_root`, until no client has
/// been connected for `idle_timeout` or `shutdown` completes. Then stops
/// every Run.
pub async fn serve(
    mut listener: Listener,
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
            accepted = transport::accept(&mut listener) => match accepted {
                Ok(Some((reader, writer))) => {
                    let core = core.clone();
                    let changed = changed.clone();
                    tokio::spawn(async move {
                        core.serve_connection(reader, writer).await;
                        changed.notify_one();
                    });
                }
                // Another user's: a client can run any command as the user it reaches the core of.
                Ok(None) => {}
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
