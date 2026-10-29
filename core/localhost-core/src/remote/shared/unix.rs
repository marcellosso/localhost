//! The shared core's transport on macOS and Linux: a Unix socket in the
//! storage root, only this user's to use.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};

use super::hash;
use crate::RunEnvironment;

pub type Listener = UnixListener;
pub(super) type Reader = OwnedReadHalf;
pub(super) type Writer = OwnedWriteHalf;

const SOCKET_NAME: &str = "localhostd.sock";

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
    std::env::temp_dir().join(format!("localhostd-{:016x}.sock", hash(storage_root)))
}

pub(super) async fn connect(storage_root: &Path) -> io::Result<(Reader, Writer)> {
    Ok(UnixStream::connect(socket_path(storage_root)).await?.into_split())
}

/// Starts the core in a session of its own, with only the session
/// variables of the client starting it.
pub(super) fn spawn_detached(
    mut daemon: std::process::Command,
    _storage_root: &Path,
) -> io::Result<std::process::Child> {
    daemon
        .current_dir("/")
        .env_clear()
        // Without an AppImage's own entries, which go away with the client.
        .envs(RunEnvironment::current().into_iter().filter(|(name, _)| {
            KEPT_VARIABLES.contains(&name.as_str()) || name.starts_with("LC_") || name.starts_with("XDG_")
        }));
    // SAFETY: `setsid` is async-signal-safe, and nothing else runs between
    // fork and exec.
    unsafe {
        daemon.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    daemon.spawn()
}

/// Listens on the socket of `storage_root`, replacing one a core that
/// ended unexpectedly left. Call it holding the lock.
pub fn bind(storage_root: &Path) -> io::Result<Listener> {
    let path = socket_path(storage_root);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// The next connection; `None` if it isn't this user's.
pub(super) async fn accept(listener: &mut Listener) -> io::Result<Option<(Reader, Writer)>> {
    let (stream, _) = listener.accept().await?;
    Ok(is_this_user(&stream).then(|| stream.into_split()))
}

/// Whether the other end runs as this user: a client can run any command
/// as the user it reaches the core of.
fn is_this_user(stream: &UnixStream) -> bool {
    // SAFETY: `geteuid` can't fail.
    stream.peer_cred().is_ok_and(|credentials| credentials.uid() == unsafe { libc::geteuid() })
}

/// Completes on SIGTERM, SIGINT or SIGHUP.
pub(super) fn told_to_end() -> io::Result<impl Future<Output = ()>> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut hang_up = signal(SignalKind::hangup())?;
    Ok(async move {
        tokio::select! {
            _ = terminate.recv() => {}
            _ = interrupt.recv() => {}
            _ = hang_up.recv() => {}
        }
    })
}

pub(super) fn clean_up(storage_root: &Path) {
    let _ = std::fs::remove_file(socket_path(storage_root));
}
