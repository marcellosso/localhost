//! The process tree of one Run: `<shell> -l -c <command line>` launched in
//! its own session, so the shell's pid is also the id of the process group
//! that holds everything it spawns (ADR-0003).
//!
//! The tree has ended once the shell has exited *and* no process is left in
//! its group. Processes still in the group when the shell exits on its own
//! are stopped (starting at SIGTERM: background jobs of a non-interactive
//! shell ignore SIGINT), so a Run never leaves anything behind.

#[cfg(unix)]
mod reader;
#[cfg(unix)]
mod spawn;
mod termination;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
use tokio::sync::oneshot;
use tokio::sync::{Mutex as AsyncMutex, Semaphore, mpsc, watch};
use tokio::time::Instant;

pub use termination::{ProcessTreeExit, StopPolicy, Termination, signal_name};

use crate::output::OutputLine;
use crate::platform::signal::{SIGINT, SIGKILL, SIGTERM};
use crate::platform::{self, Pid, process_group_exists};
use crate::{LocalhostError, RunEnvironment};

/// Everything needed to launch one Run's process tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchConfiguration {
    pub command_line: String,
    pub working_directory: PathBuf,
    /// The complete environment of the Run (see `RunEnvironment::merged`).
    pub environment: BTreeMap<String, String>,
    /// The shell the command line runs through, as `<shell> -l -c <command line>`.
    pub shell: String,
}

impl LaunchConfiguration {
    /// A configuration whose shell is the environment's `$SHELL`.
    pub fn new(
        command_line: impl Into<String>,
        working_directory: impl Into<PathBuf>,
        environment: BTreeMap<String, String>,
    ) -> LaunchConfiguration {
        let shell = RunEnvironment::shell(&environment);
        LaunchConfiguration {
            command_line: command_line.into(),
            working_directory: working_directory.into(),
            environment,
            shell,
        }
    }
}

/// A line longer than this is delivered in pieces, so output without
/// newlines (progress bars) can't grow the buffer without bound.
pub const MAX_LINE_LENGTH: usize = 64 * 1024;

/// How often to check whether the process group is gone.
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to wait for the pipes to drain once the group is gone. A process
/// that left the group may still hold them open.
#[cfg(unix)]
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// A running (or ended) process tree. Cheap to clone; every clone is the same tree.
///
/// Must be launched inside a multi-threaded tokio runtime.
#[derive(Clone)]
pub struct ProcessTree {
    inner: Arc<Inner>,
}

struct Inner {
    pid: Pid,
    stop_policy: StopPolicy,
    output: AsyncMutex<mpsc::UnboundedReceiver<Vec<OutputLine>>>,
    flow: Arc<Semaphore>,
    shell_termination: watch::Sender<Option<Termination>>,
    exit: watch::Sender<Option<ProcessTreeExit>>,
    escalation: Mutex<Escalation>,
}

#[derive(Default)]
struct Escalation {
    is_escalating: bool,
    strongest_signal_sent: Option<i32>,
}

impl ProcessTree {
    /// Launches the command line. Fails before anything runs if the working
    /// directory or the shell is missing.
    pub fn launch(configuration: &LaunchConfiguration, stop_policy: StopPolicy) -> Result<ProcessTree, LocalhostError> {
        #[cfg(not(unix))]
        {
            let _ = (configuration, stop_policy);
            Err(LocalhostError::ExecutionUnavailable)
        }
        #[cfg(unix)]
        Self::launch_on_unix(configuration, stop_policy)
    }

    #[cfg(unix)]
    fn launch_on_unix(
        configuration: &LaunchConfiguration,
        stop_policy: StopPolicy,
    ) -> Result<ProcessTree, LocalhostError> {
        use crate::output::OutputChannel;

        if !configuration.working_directory.is_dir() {
            return Err(LocalhostError::WorkingDirectoryMissing(
                configuration.working_directory.to_string_lossy().into_owned(),
            ));
        }
        if !std::path::Path::new(&configuration.shell).exists() {
            return Err(LocalhostError::ShellNotFound(configuration.shell.clone()));
        }

        let stdout = spawn::pipe()?;
        let stderr = spawn::pipe()?;
        let pid = spawn::spawn(configuration, &stdout.write, &stderr.write)?;
        // Our copies of the write ends must close, or the pipes never end.
        drop((stdout.write, stderr.write));

        let (batches, output) = mpsc::unbounded_channel();
        let flow = Arc::new(Semaphore::new(reader::PENDING_BATCHES));
        let (cancel, cancelled) = watch::channel(false);
        let readers =
            [(stdout.read, OutputChannel::Stdout), (stderr.read, OutputChannel::Stderr)].map(|(fd, channel)| {
                tokio::spawn(reader::read_lines(fd, channel, batches.clone(), flow.clone(), cancelled.clone()))
            });
        drop(batches);

        let tree = ProcessTree {
            inner: Arc::new(Inner {
                pid,
                stop_policy,
                output: AsyncMutex::new(output),
                flow,
                shell_termination: watch::channel(None).0,
                exit: watch::channel(None).0,
                escalation: Mutex::new(Escalation::default()),
            }),
        };
        let shell_status = wait_for_shell(pid);
        tokio::spawn(tree.clone().monitor(shell_status, readers, cancel));
        Ok(tree)
    }

    /// The shell's pid, which is also the process group and session id.
    pub fn pid(&self) -> Pid {
        self.inner.pid
    }

    /// The next batch of output lines, in arrival order, as read from the
    /// pipes (so chatty commands cost one batch per read, not per line).
    /// Reading pauses while batches aren't taken. `None` once the tree has
    /// ended and every line was taken.
    pub async fn next_output(&self) -> Option<Vec<OutputLine>> {
        let batch = self.inner.output.lock().await.recv().await;
        if batch.is_some() {
            self.inner.flow.add_permits(1);
        }
        batch
    }

    /// Waits until the shell has exited. Processes it left in the group may
    /// still be running; `wait_for_exit` waits for those too.
    pub async fn wait_for_shell_exit(&self) -> Termination {
        let mut receiver = self.inner.shell_termination.subscribe();
        let termination = receiver.wait_for(Option::is_some).await.expect("the tree owns the sender");
        termination.expect("waited for Some")
    }

    /// Waits until the tree has ended.
    pub async fn wait_for_exit(&self) -> ProcessTreeExit {
        let mut receiver = self.inner.exit.subscribe();
        let exit = receiver.wait_for(Option::is_some).await.expect("the tree owns the sender");
        exit.expect("waited for Some")
    }

    /// The exit, if the tree has ended.
    pub fn exit(&self) -> Option<ProcessTreeExit> {
        *self.inner.exit.borrow()
    }

    /// Stops the whole process group: SIGINT, then SIGTERM, then SIGKILL, each
    /// sent only if the group outlives the previous one's timeout. Returns
    /// once the tree has ended. Safe to call repeatedly and concurrently.
    pub async fn stop(&self) -> ProcessTreeExit {
        if let Some(exit) = self.exit() {
            return exit;
        }
        self.escalate(SIGINT).await;
        self.wait_for_exit().await
    }

    #[cfg(unix)]
    async fn monitor(
        self,
        shell_status: oneshot::Receiver<i32>,
        readers: [tokio::task::JoinHandle<()>; 2],
        cancel: watch::Sender<bool>,
    ) {
        let status = shell_status.await.unwrap_or(0);
        let termination = Termination::from_wait_status(status);
        self.inner.shell_termination.send_replace(Some(termination));

        // Anything still in the group is left over from the shell: stop it.
        if process_group_exists(self.inner.pid) {
            self.escalate(SIGTERM).await;
        }
        while process_group_exists(self.inner.pid) {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        // Wait for both pipes to end, cancelling the readers after the timeout.
        let [stdout, stderr] = readers;
        let drained = async {
            let _ = stdout.await;
            let _ = stderr.await;
        };
        tokio::pin!(drained);
        if tokio::time::timeout(OUTPUT_DRAIN_TIMEOUT, &mut drained).await.is_err() {
            cancel.send_replace(true);
            drained.await;
        }

        let strongest_signal_sent = self.inner.escalation.lock().unwrap().strongest_signal_sent;
        self.inner.exit.send_replace(Some(ProcessTreeExit { termination, strongest_signal_sent }));
    }

    /// Sends the escalating signals, beginning with `first_signal`, until the
    /// group is gone. Only one escalation runs at a time; later callers return
    /// immediately.
    async fn escalate(&self, first_signal: i32) {
        {
            let mut escalation = self.inner.escalation.lock().unwrap();
            if escalation.is_escalating {
                return;
            }
            escalation.is_escalating = true;
        }
        let policy = self.inner.stop_policy;
        let steps =
            [(SIGINT, Some(policy.interrupt_timeout)), (SIGTERM, Some(policy.terminate_timeout)), (SIGKILL, None)];
        for (signal, timeout) in steps.into_iter().skip_while(|(signal, _)| *signal != first_signal) {
            if !process_group_exists(self.inner.pid) {
                return;
            }
            self.inner.escalation.lock().unwrap().strongest_signal_sent = Some(signal);
            platform::signal_process_group(self.inner.pid, signal);
            if self.wait_for_process_group_to_end(timeout).await {
                return;
            }
        }
    }

    /// Whether the group ended within `timeout` (`None` waits forever).
    async fn wait_for_process_group_to_end(&self, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        while process_group_exists(self.inner.pid) {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return false;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        true
    }
}

#[cfg(unix)]
/// Blocks a thread in `waitpid` until the shell exits, and reaps it.
fn wait_for_shell(pid: Pid) -> oneshot::Receiver<i32> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name(format!("localhost-wait-{pid}"))
        .spawn(move || {
            let mut status = 0;
            while unsafe { libc::waitpid(pid, &mut status, 0) } == -1 && spawn::errno() == libc::EINTR {}
            let _ = sender.send(status);
        })
        .expect("a thread to wait for the shell");
    receiver
}
