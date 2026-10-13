use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::LocalhostError;

/// How the command's shell ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Termination {
    Exited { code: i32 },
    Signaled { signal: i32 },
}

impl Termination {
    /// Decodes a `waitpid` status.
    pub(crate) fn from_wait_status(status: i32) -> Termination {
        if libc::WIFSIGNALED(status) {
            Termination::Signaled { signal: libc::WTERMSIG(status) }
        } else {
            Termination::Exited { code: libc::WEXITSTATUS(status) }
        }
    }

    /// A short human-readable summary, e.g. "Exited with code 1".
    pub fn summary(&self) -> String {
        match self {
            Termination::Exited { code } => format!("Exited with code {code}"),
            Termination::Signaled { signal } => format!("Terminated by signal {}", signal_name(*signal)),
        }
    }

    /// The launch failure this termination stands for, by shell convention.
    pub fn launch_error(&self) -> Option<LocalhostError> {
        match self {
            Termination::Exited { code: 127 } => Some(LocalhostError::CommandNotFound),
            Termination::Exited { code: 126 } => Some(LocalhostError::CommandNotExecutable),
            _ => None,
        }
    }
}

/// The conventional name of a signal, e.g. "SIGTERM".
pub fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGHUP => "SIGHUP".into(),
        libc::SIGINT => "SIGINT".into(),
        libc::SIGQUIT => "SIGQUIT".into(),
        libc::SIGABRT => "SIGABRT".into(),
        libc::SIGKILL => "SIGKILL".into(),
        libc::SIGBUS => "SIGBUS".into(),
        libc::SIGSEGV => "SIGSEGV".into(),
        libc::SIGPIPE => "SIGPIPE".into(),
        libc::SIGTERM => "SIGTERM".into(),
        other => other.to_string(),
    }
}

/// How a process tree ended: the shell's termination, and how hard localhost
/// had to push to end the whole group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessTreeExit {
    pub termination: Termination,
    /// The last signal localhost sent to the process group (`SIGINT`,
    /// `SIGTERM` or `SIGKILL`), or `None` if the tree ended on its own.
    pub strongest_signal_sent: Option<i32>,
}

impl ProcessTreeExit {
    pub fn was_force_killed(&self) -> bool {
        self.strongest_signal_sent == Some(libc::SIGKILL)
    }
}

/// How long to wait after each signal before escalating to the next. The
/// waits use the monotonic clock, which stops while the machine sleeps: a
/// process frozen by sleep gets its full time once it can run again, instead
/// of being escalated on wake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StopPolicy {
    pub interrupt_timeout: Duration,
    pub terminate_timeout: Duration,
}

impl Default for StopPolicy {
    /// SIGINT → 5s → SIGTERM → 3s → SIGKILL.
    fn default() -> Self {
        StopPolicy { interrupt_timeout: Duration::from_secs(5), terminate_timeout: Duration::from_secs(3) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_and_ordinary_failures_are_not_launch_errors() {
        assert_eq!(Termination::Exited { code: 0 }.launch_error(), None);
        assert_eq!(Termination::Exited { code: 1 }.launch_error(), None);
        assert_eq!(Termination::Signaled { signal: libc::SIGKILL }.launch_error(), None);
        assert_eq!(Termination::Exited { code: 127 }.launch_error(), Some(LocalhostError::CommandNotFound));
        assert_eq!(Termination::Exited { code: 126 }.launch_error(), Some(LocalhostError::CommandNotExecutable));
    }

    #[test]
    fn summaries_name_the_code_or_signal() {
        assert_eq!(Termination::Exited { code: 1 }.summary(), "Exited with code 1");
        assert_eq!(Termination::Signaled { signal: libc::SIGTERM }.summary(), "Terminated by signal SIGTERM");
    }

    #[test]
    fn the_default_policy_waits_5s_after_sigint_and_3s_after_sigterm() {
        let policy = StopPolicy::default();
        assert_eq!(policy.interrupt_timeout, Duration::from_secs(5));
        assert_eq!(policy.terminate_timeout, Duration::from_secs(3));
    }
}
