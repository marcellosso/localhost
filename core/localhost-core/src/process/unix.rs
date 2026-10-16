//! Launching and signalling a Run's process group on macOS and Linux.

use std::time::Duration;

use tokio::net::unix::pipe;
use tokio::sync::oneshot;

use super::{LaunchConfiguration, spawn};
use crate::LocalhostError;
use crate::platform::signal::{SIGINT, SIGKILL, SIGTERM};
use crate::platform::{self, Pid};
use crate::process::Termination;

/// The signals a stop escalates through, in order.
pub(super) const STOP_SIGNALS: [i32; 3] = [SIGINT, SIGTERM, SIGKILL];
/// Where stopping what the shell left behind starts: background jobs of a
/// non-interactive shell ignore SIGINT.
pub(super) const LEFTOVER_SIGNAL: i32 = SIGTERM;
/// How long the group may take to empty after the shell exits before what
/// is in it counts as left over: the kernel reports a reaped shell's group
/// at once.
pub(super) const LEFTOVER_GRACE: Duration = Duration::ZERO;

/// The Run's process group, whose id is the shell's pid.
pub(super) struct Group(Pid);

impl Group {
    /// Whether any process is left in the group (a zombie still counts).
    pub(super) fn exists(&self) -> bool {
        platform::process_group_exists(self.0)
    }

    pub(super) fn send(&self, signal: i32) {
        platform::signal_process_group(self.0, signal)
    }
}

pub(super) struct Launched {
    pub(super) pid: Pid,
    pub(super) group: Group,
    pub(super) stdout: pipe::Receiver,
    pub(super) stderr: pipe::Receiver,
    pub(super) shell_termination: oneshot::Receiver<Termination>,
}

pub(super) fn launch(configuration: &LaunchConfiguration) -> Result<Launched, LocalhostError> {
    let stdout = spawn::pipe()?;
    let stderr = spawn::pipe()?;
    let receiver = |fd| pipe::Receiver::from_owned_fd(fd).map_err(|error| launch_failed(&error));
    let (stdout_read, stderr_read) = (receiver(stdout.read)?, receiver(stderr.read)?);
    let pid = spawn::spawn(configuration, &stdout.write, &stderr.write)?;
    // Our copies of the write ends must close, or the pipes never end.
    drop((stdout.write, stderr.write));
    Ok(Launched {
        pid,
        group: Group(pid),
        stdout: stdout_read,
        stderr: stderr_read,
        shell_termination: wait_for_shell(pid),
    })
}

fn launch_failed(error: &std::io::Error) -> LocalhostError {
    LocalhostError::LaunchFailed { errno: error.raw_os_error().unwrap_or(0) }
}

/// Blocks a thread in `waitpid` until the shell exits, and reaps it.
fn wait_for_shell(pid: Pid) -> oneshot::Receiver<Termination> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name(format!("localhost-wait-{pid}"))
        .spawn(move || {
            let mut status = 0;
            while unsafe { libc::waitpid(pid, &mut status, 0) } == -1 && spawn::errno() == libc::EINTR {}
            let _ = sender.send(Termination::from_wait_status(status));
        })
        .expect("a thread to wait for the shell");
    receiver
}
