//! Machines: other computers whose localhost this app shows, reached over
//! SSH as the user already reaches them (`~/.ssh/config`, keys, the agent,
//! Tailscale or any other network). Each Machine keeps its own Project
//! Configuration, Secrets, shared core and Runs; this app views, controls
//! and edits them there. A Secret typed here is sent to the Machine that
//! keeps it, and never stored here.
//!
//! The app runs `ssh <destination> localhost remote --stdio` (`ssh`), which
//! serves the other Machine's Projects, statuses and logs over the Machine
//! protocol (`protocol`, `relay`), and talks to it through a
//! [`MachineConnection`]. That SSH connection is also a ControlMaster, so a
//! Port of the other Machine is forwarded to this one over it, and every
//! Forward ends with it. Nothing listens on the network that didn't before:
//! the other Machine's shared core stays on its own user's socket.

mod connection;
mod machines;
pub(crate) mod protocol;
pub mod relay;
mod ssh;
mod ssh_config;

use serde::{Deserialize, Serialize};

pub use connection::MachineConnection;
pub use machines::{MachineSnapshot, MachineState, Machines, MachinesEvent};
pub use protocol::{MACHINE_PROTOCOL_VERSION, MANAGES_PROJECTS, MachineHello, ProjectFileText};
pub use ssh::{Forward, SshLink};
pub use ssh_config::ssh_config_hosts;

/// Why another Machine can't be reached, as the app explains it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MachineProblem {
    /// There is no `ssh` command on this computer.
    SshNotInstalled,
    /// Its host key isn't in `known_hosts` yet. localhost never trusts one
    /// on the user's behalf.
    HostKeyNotTrusted,
    /// Its host key differs from the one in `known_hosts`.
    HostKeyChanged,
    /// SSH would have to ask for a password or passphrase, and can't.
    SignInNeedsPrompt,
    HostNotFound,
    /// The network refused or timed out: SSH's words.
    Unreachable(String),
    /// No `localhost` command there, or not on the `PATH` SSH gives.
    NotInstalled,
    /// Its localhost has no `localhost remote` (the version it reported, if any).
    TooOld(Option<String>),
    /// localhost there started but couldn't serve this app, in its own
    /// words, e.g. its shared core is another version's.
    CantServe(String),
    /// It was connected and the connection ended.
    Lost,
    /// Anything else SSH said, e.g. a mistake in `~/.ssh/config`. Not
    /// tried again on its own.
    Other(String),
}

impl MachineProblem {
    /// What went wrong reaching `machine`, and what to do next.
    pub fn message(&self, machine: &str) -> String {
        use MachineProblem::*;
        match self {
            SshNotInstalled => "There is no ssh command on this computer. Install OpenSSH, then try again.".into(),
            HostKeyNotTrusted => format!(
                "{machine}’s host key isn’t trusted yet. Connect once from a terminal with `ssh {machine}`, then try again."
            ),
            HostKeyChanged => format!(
                "{machine}’s host key changed. If you expected that, update ~/.ssh/known_hosts from a terminal; otherwise don’t connect."
            ),
            SignInNeedsPrompt => format!(
                "SSH can’t sign in to {machine} without asking for a password or passphrase. Add your key to the SSH agent with ssh-add, then try again."
            ),
            HostNotFound => {
                format!("{machine} wasn’t found. Check its name in ~/.ssh/config, and that you’re on its network.")
            }
            Unreachable(reason) => format!("localhost can’t reach {machine}: {reason}."),
            NotInstalled => format!(
                "localhost isn’t installed on {machine}, or isn’t on its PATH. Install it there, then try again."
            ),
            TooOld(Some(version)) => {
                format!(
                    "{machine} runs localhost {version}, which can’t be viewed from another Machine. Update localhost there."
                )
            }
            TooOld(None) => {
                format!("{machine} runs a localhost that can’t be viewed from another Machine. Update localhost there.")
            }
            CantServe(reason) => format!("localhost on {machine} couldn’t start: {reason}"),
            Lost => format!("The connection to {machine} was lost."),
            Other(said) => format!("SSH couldn’t connect to {machine}: {said}"),
        }
    }

    /// What SSH's standard error (`said`) and exit `code` mean, after the
    /// connection ended before the other end said hello.
    pub(crate) fn from_ssh(said: &str, code: Option<i32>) -> MachineProblem {
        let lower = said.to_lowercase();
        let last_line = said.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
        if lower.contains("remote host identification has changed") {
            MachineProblem::HostKeyChanged
        } else if lower.contains("host key verification failed") {
            MachineProblem::HostKeyNotTrusted
        } else if lower.contains("permission denied") {
            MachineProblem::SignInNeedsPrompt
        } else if lower.contains("could not resolve hostname") {
            MachineProblem::HostNotFound
        } else if lower.contains("connection refused")
            || lower.contains("timed out")
            || lower.contains("no route to host")
            || lower.contains("network is unreachable")
            || lower.contains("connection reset")
            || lower.contains("connection closed by")
        {
            MachineProblem::Unreachable(last_line.trim_start_matches("ssh: ").to_string())
        } else if code == Some(127)
            || lower.contains("localhost: command not found")
            || lower.contains("localhost: not found")
        {
            MachineProblem::NotInstalled
        } else if lower.contains("unknown command `remote`") {
            MachineProblem::TooOld(None)
        } else if last_line.is_empty() {
            MachineProblem::Lost
        } else {
            MachineProblem::Other(last_line.to_string())
        }
    }
}

/// Another Machine, as this app remembers it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Machine {
    pub id: crate::Id,
    /// What the app calls it: by default its destination.
    pub name: String,
    /// What `ssh` connects to: a `Host` of `~/.ssh/config`, or `user@host`.
    pub destination: String,
}

#[cfg(test)]
mod tests {
    use super::MachineProblem::{self, *};

    #[test]
    fn reads_what_ssh_says() {
        let problem = |said: &str, code| MachineProblem::from_ssh(said, code);
        assert_eq!(problem("Host key verification failed.\r\n", Some(255)), HostKeyNotTrusted);
        assert_eq!(
            problem(
                "@@@@\n@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\nHost key verification failed.",
                Some(255)
            ),
            HostKeyChanged
        );
        assert_eq!(problem("marcel@devbox: Permission denied (publickey,password).", Some(255)), SignInNeedsPrompt);
        assert_eq!(
            problem("ssh: Could not resolve hostname devbx: Name or service not known", Some(255)),
            HostNotFound
        );
        assert_eq!(
            problem("ssh: connect to host 10.0.0.9 port 22: Connection refused", Some(255)),
            Unreachable("connect to host 10.0.0.9 port 22: Connection refused".into())
        );
        assert_eq!(problem("bash: line 1: localhost: command not found", Some(127)), NotInstalled);
        assert_eq!(problem("Unknown command `remote`.\nRun `localhost --help` for usage.", Some(2)), TooOld(None));
        assert_eq!(
            problem("kex_exchange_identification: read: Connection reset by peer", Some(255)),
            Unreachable("kex_exchange_identification: read: Connection reset by peer".into())
        );
        assert_eq!(
            problem("/Users/me/.ssh/config line 4: Bad configuration option: hostnme", Some(255)),
            Other("/Users/me/.ssh/config line 4: Bad configuration option: hostnme".into())
        );
        assert_eq!(problem("", Some(255)), Lost);
    }
}
