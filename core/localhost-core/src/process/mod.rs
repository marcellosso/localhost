//! The process tree of one Run: `<shell> -l -c <command line>` launched in
//! its own session, so the shell's pid is also the id of the process group
//! that holds everything it spawns (ADR-0003). On Windows, `cmd.exe /s /c
//! <command line>` in its own Job Object (ADR-0010, ADR-0011), which is what
//! "the group" means there.
//!
//! The tree has ended once the shell has exited *and* no process is left in
//! its group. Processes still in the group when the shell exits on its own
//! are stopped (on Unix starting at SIGTERM: background jobs of a
//! non-interactive shell ignore SIGINT), so a Run never leaves anything behind.

mod reader;
#[cfg(unix)]
mod spawn;
mod termination;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Mutex as AsyncMutex, Semaphore, mpsc, oneshot, watch};
use tokio::time::Instant;

pub use termination::{ProcessTreeExit, StopPolicy, Termination, signal_name};

use crate::output::{OutputChannel, OutputLine};
use crate::platform::signal::{SIGINT, SIGTERM};
use crate::platform::{ListeningProcess, Pid};
use crate::{LocalhostError, RunEnvironment};

/// Everything needed to launch one Run's process tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchConfiguration {
    pub command_line: String,
    pub working_directory: PathBuf,
    /// The complete environment of the Run (see `RunEnvironment::merged`).
    pub environment: BTreeMap<String, String>,
    /// The shell the command line runs through, as `<shell> -l -c <command
    /// line>` (on Windows, `cmd.exe`, as `<shell> /s /c "<command line>"`).
    pub shell: String,
}

impl LaunchConfiguration {
    /// A configuration whose shell is the environment's `$SHELL` (`%ComSpec%` on Windows).
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
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// What a Run's process tree uses at one moment (ADR-0012).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeUsage {
    /// The tree's live processes.
    pub process_count: u32,
    /// CPU time used by the tree so far. On Windows the job's total; on
    /// macOS and Linux the live processes' own and that of the children
    /// they have waited for.
    pub cpu_time: Duration,
    /// The memory of the live processes (see `platform::ProcessUsage`).
    pub memory_bytes: u64,
}

impl TreeUsage {
    fn add(&mut self, process: crate::platform::ProcessUsage) {
        self.process_count += 1;
        self.cpu_time += process.cpu_time;
        self.memory_bytes += process.memory_bytes;
    }
}

/// A running (or ended) process tree. Cheap to clone; every clone is the same tree.
///
/// Must be launched inside a multi-threaded tokio runtime.
#[derive(Clone)]
pub struct ProcessTree {
    inner: Arc<Inner>,
}

struct Inner {
    pid: Pid,
    group: imp::Group,
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
        if !configuration.working_directory.is_dir() {
            return Err(LocalhostError::WorkingDirectoryMissing(
                configuration.working_directory.to_string_lossy().into_owned(),
            ));
        }
        if !std::path::Path::new(&configuration.shell).exists() {
            return Err(LocalhostError::ShellNotFound(configuration.shell.clone()));
        }

        let launched = imp::launch(configuration)?;
        let (batches, output) = mpsc::unbounded_channel();
        let flow = Arc::new(Semaphore::new(reader::PENDING_BATCHES));
        let (cancel, cancelled) = watch::channel(false);
        let readers = [
            tokio::spawn(reader::read_lines(
                launched.stdout,
                OutputChannel::Stdout,
                batches.clone(),
                flow.clone(),
                cancelled.clone(),
            )),
            tokio::spawn(reader::read_lines(
                launched.stderr,
                OutputChannel::Stderr,
                batches.clone(),
                flow.clone(),
                cancelled.clone(),
            )),
        ];
        drop(batches);

        let tree = ProcessTree {
            inner: Arc::new(Inner {
                pid: launched.pid,
                group: launched.group,
                stop_policy,
                output: AsyncMutex::new(output),
                flow,
                shell_termination: watch::channel(None).0,
                exit: watch::channel(None).0,
                escalation: Mutex::new(Escalation::default()),
            }),
        };
        tokio::spawn(tree.clone().monitor(launched.shell_termination, readers, cancel));
        Ok(tree)
    }

    /// The shell's pid, which is also the process group (and on Unix the session) id.
    pub fn pid(&self) -> Pid {
        self.inner.pid
    }

    /// Whether a listening process belongs to this tree.
    pub(crate) fn contains(&self, process: &ListeningProcess) -> bool {
        self.inner.group.contains(process)
    }

    /// What the tree's live processes use now: every process in the Run's
    /// session or process group and their descendants (on Windows, its Job
    /// Object). Blocking.
    pub(crate) fn usage(&self) -> TreeUsage {
        self.inner.group.usage()
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

    pub async fn wait_for_exit(&self) -> ProcessTreeExit {
        let mut receiver = self.inner.exit.subscribe();
        let exit = receiver.wait_for(Option::is_some).await.expect("the tree owns the sender");
        exit.expect("waited for Some")
    }

    pub fn exit(&self) -> Option<ProcessTreeExit> {
        *self.inner.exit.borrow()
    }

    /// Stops the whole process group: SIGINT, then SIGTERM, then SIGKILL, each
    /// sent only if the group outlives the previous one's timeout (on Windows,
    /// Ctrl-Break, then terminating the job). Returns once the tree has ended.
    /// Safe to call repeatedly and concurrently.
    pub async fn stop(&self) -> ProcessTreeExit {
        if let Some(exit) = self.exit() {
            return exit;
        }
        self.escalate(SIGINT).await;
        self.wait_for_exit().await
    }

    async fn monitor(
        self,
        shell_termination: oneshot::Receiver<Termination>,
        readers: [tokio::task::JoinHandle<()>; 2],
        cancel: watch::Sender<bool>,
    ) {
        let termination = shell_termination.await.unwrap_or(Termination::Exited { code: 0 });
        self.inner.shell_termination.send_replace(Some(termination));

        // Anything still in the group is left over from the shell: stop it.
        if !self.wait_for_process_group_to_end(Some(imp::LEFTOVER_GRACE)).await {
            self.escalate(imp::LEFTOVER_SIGNAL).await;
        }
        while self.inner.group.exists() {
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
        for signal in imp::STOP_SIGNALS.into_iter().skip_while(|signal| *signal != first_signal) {
            if !self.inner.group.exists() {
                return;
            }
            let timeout = match signal {
                SIGINT => Some(policy.interrupt_timeout),
                SIGTERM => Some(policy.terminate_timeout),
                _ => None,
            };
            self.inner.escalation.lock().unwrap().strongest_signal_sent = Some(signal);
            self.inner.group.send(signal);
            if self.wait_for_process_group_to_end(timeout).await {
                return;
            }
        }
    }

    /// Whether the group ended within `timeout` (`None` waits forever).
    async fn wait_for_process_group_to_end(&self, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        while self.inner.group.exists() {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return false;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        true
    }
}
