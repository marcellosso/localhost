//! Reaching another Machine with the system's `ssh`, so `~/.ssh/config`,
//! keys, the agent, `ProxyJump` and whatever network the user has all
//! apply, and localhost adds no sign-in of its own.
//!
//! One `ssh` process per Machine runs `localhost remote --stdio` there and
//! is also a ControlMaster: Forwards are added to it with `ssh -O forward`,
//! and end with it. Its socket is in a folder only this user can open,
//! since whoever reaches it reaches the other Machine.
//!
//! `ssh` never asks anything (`BatchMode`), and host keys are checked as
//! the user configured: an unknown one is a `MachineProblem`, never trusted
//! on the user's behalf. Forwards from the user's own configuration are
//! cleared, and only loopback addresses here are ever bound.

use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use super::{Machine, MachineConnection, MachineProblem};
use crate::LocalhostError;

/// What runs on the other Machine.
const REMOTE_COMMAND: &str = "localhost remote --stdio";

/// How many of SSH's last lines of standard error are kept, to explain a failure.
const KEPT_LINES: usize = 20;

/// The options every `ssh` gets, besides the destination.
fn options() -> Vec<String> {
    ["BatchMode=yes", "ForwardAgent=no", "ForwardX11=no", "LogLevel=ERROR"]
        .into_iter()
        .flat_map(|option| ["-o".to_string(), option.to_string()])
        .collect()
}

/// The master's own options besides. It clears the forwards of the user's
/// configuration, so only those localhost asks for are made; the control
/// commands mustn't, since that clears their own `-L` too.
fn master_options() -> Vec<String> {
    [
        "ServerAliveInterval=15",
        "ServerAliveCountMax=3",
        "ConnectTimeout=15",
        "ClearAllForwardings=yes",
        "ControlPersist=no",
    ]
    .into_iter()
    .flat_map(|option| ["-o".to_string(), option.to_string()])
    .collect()
}

/// A Port of another Machine, forwarded to a port of this one: the same,
/// unless that one was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Forward {
    pub port: u16,
    pub local_port: u16,
}

/// The `ssh` process of one Machine, and its Forwards. Dropping it ends the
/// connection and every Forward.
pub struct SshLink {
    machine: Machine,
    ssh: PathBuf,
    control_folder: PathBuf,
    /// The control socket's name, relative to `control_folder`.
    control_name: String,
    child: Mutex<Option<Child>>,
    forwards: Mutex<HashSet<Forward>>,
}

impl SshLink {
    /// Connects to `machine` with the `ssh` at `ssh`, keeping its control
    /// socket in `control_folder`. Fails with what SSH said.
    pub async fn connect(
        machine: Machine,
        ssh: &Path,
        control_folder: &Path,
    ) -> Result<(Arc<SshLink>, MachineConnection), LocalhostError> {
        let unavailable = |problem| LocalhostError::MachineUnavailable { machine: machine.name.clone(), problem };
        create_private_folder(control_folder).map_err(|error| unavailable(MachineProblem::Other(error.to_string())))?;
        let control_name = machine.id.to_string().split('-').next().unwrap_or("machine").to_string();
        let _ = std::fs::remove_file(control_folder.join(&control_name));

        let mut command = Command::new(ssh);
        // The socket is named relative to the folder: a Unix socket's path
        // is limited to about 104 bytes, and `ssh` adds 17 while making it,
        // which a temporary or home folder can exceed (macOS's do).
        command
            .current_dir(control_folder)
            .arg("-T")
            .arg("-M")
            .arg("-S")
            .arg(&control_name)
            .args(options())
            .args(master_options())
            .arg("--")
            .arg(ssh_destination(&machine.destination))
            .arg(REMOTE_COMMAND)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => unavailable(MachineProblem::SshNotInstalled),
            _ => unavailable(MachineProblem::Other(error.to_string())),
        })?;
        let (Some(input), Some(output), Some(errors)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(unavailable(MachineProblem::Other("its input and output aren’t available".into())));
        };
        let said = Arc::new(Mutex::new(Vec::<String>::new()));
        let kept = said.clone();
        let stderr = tokio::spawn(async move {
            let mut lines = BufReader::new(errors).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut kept = kept.lock().unwrap();
                kept.push(line);
                if kept.len() > KEPT_LINES {
                    kept.remove(0);
                }
            }
        });

        match MachineConnection::connect(&machine.name, output, input).await {
            Ok(connection) => {
                let link = Arc::new(SshLink {
                    machine,
                    ssh: ssh.to_path_buf(),
                    control_folder: control_folder.to_path_buf(),
                    control_name,
                    child: Mutex::new(Some(child)),
                    forwards: Mutex::new(HashSet::new()),
                });
                // The process ends when the connection does, and the reverse.
                let watched = Arc::downgrade(&link);
                let closed = connection.clone();
                tokio::spawn(async move {
                    closed.closed().await;
                    if let Some(link) = watched.upgrade() {
                        link.end();
                    }
                });
                Ok((link, connection))
            }
            Err(_) => {
                let code = child.wait().await.ok().and_then(|status| status.code());
                let _ = stderr.await;
                let said = said.lock().unwrap().join("\n");
                Err(unavailable(MachineProblem::from_ssh(&said, code)))
            }
        }
    }

    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// Forwards `port` of the other Machine to the same port here. The
    /// same number, because `*.localhost` names, OAuth callbacks and
    /// cookies expect it.
    pub async fn forward(&self, port: u16) -> Result<Forward, LocalhostError> {
        self.forward_to(port, port).await
    }

    /// Forwards `port` of the other Machine to `local_port` on this
    /// computer's loopback addresses. The other end is `localhost` as that
    /// Machine resolves it, so a Service listening on either of its loopback
    /// addresses, or on every address, is reached.
    pub async fn forward_to(&self, port: u16, local_port: u16) -> Result<Forward, LocalhostError> {
        let forward = Forward { port, local_port };
        if self.forwards.lock().unwrap().contains(&forward) {
            return Ok(forward);
        }
        // `ssh` would only report a taken port in the master's output, and
        // on macOS could bind beside a Service listening on every address,
        // hiding it from this computer's browser.
        if !is_free(local_port) || crate::port_probe::is_accepting(local_port).await {
            let owner = tokio::task::spawn_blocking(move || crate::listener::holding(local_port)).await.ok().flatten();
            return Err(LocalhostError::PortConflict { port: local_port, owner: owner.map(Box::new) });
        }
        self.control("forward", &format!("127.0.0.1:{local_port}:localhost:{port}"), port).await?;
        // Browsers may try `::1` first for `localhost`. Not every computer has it.
        if TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, local_port))).is_ok() {
            let _ = self.control("forward", &format!("[::1]:{local_port}:localhost:{port}"), port).await;
        }
        self.forwards.lock().unwrap().insert(forward);
        Ok(forward)
    }

    /// Ends `forward`, if it is one of this connection's.
    pub async fn cancel_forward(&self, forward: Forward) {
        if self.forwards.lock().unwrap().remove(&forward) {
            let Forward { port, local_port } = forward;
            let _ = self.control("cancel", &format!("127.0.0.1:{local_port}:localhost:{port}"), port).await;
            let _ = self.control("cancel", &format!("[::1]:{local_port}:localhost:{port}"), port).await;
        }
    }

    pub fn forwards(&self) -> Vec<Forward> {
        let mut forwards: Vec<Forward> = self.forwards.lock().unwrap().iter().copied().collect();
        forwards.sort_unstable_by_key(|forward| (forward.port, forward.local_port));
        forwards
    }

    /// Ends the connection, and with it every Forward.
    pub fn end(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.start_kill();
        }
        self.forwards.lock().unwrap().clear();
        let _ = std::fs::remove_file(self.control_folder.join(&self.control_name));
    }

    /// Asks the master to `operation` (`forward`, `cancel`) the local forward `spec`.
    async fn control(&self, operation: &str, spec: &str, port: u16) -> Result<(), LocalhostError> {
        let output = Command::new(&self.ssh)
            .current_dir(&self.control_folder)
            .arg("-S")
            .arg(&self.control_name)
            .arg("-O")
            .arg(operation)
            .arg("-L")
            .arg(spec)
            .args(options())
            .arg("--")
            .arg(ssh_destination(&self.machine.destination))
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|error| LocalhostError::ForwardFailed { port, reason: error.to_string() })?;
        if output.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&output.stderr);
        let reason = said.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("ssh failed").trim();
        Err(LocalhostError::ForwardFailed { port, reason: reason.to_string() })
    }
}

impl Drop for SshLink {
    fn drop(&mut self) {
        self.end();
    }
}

/// What `ssh` is given for a Machine's `destination`: `user@host:port`, as
/// people write it, in the `ssh://` form `ssh` reads a port from; a `Host`
/// of `~/.ssh/config` or `user@host` as it is.
fn ssh_destination(destination: &str) -> String {
    let Some((host, port)) = destination.rsplit_once(':') else { return destination.to_string() };
    let is_port = !port.is_empty() && port.len() <= 5 && port.chars().all(|character| character.is_ascii_digit());
    let address = host.rsplit_once('@').map_or(host, |(_, address)| address);
    // An IPv6 address only with brackets: `[::1]:2222`.
    let is_host =
        !address.is_empty() && (!address.contains(':') || (address.starts_with('[') && address.ends_with(']')));
    if destination.starts_with("ssh://") || !is_port || !is_host {
        return destination.to_string();
    }
    format!("ssh://{destination}")
}

/// Whether nothing on this computer listens on `port` of IPv4's loopback.
fn is_free(port: u16) -> bool {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok()
}

fn create_private_folder(folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ssh_destination;

    #[test]
    fn gives_ssh_a_port_in_the_form_it_reads() {
        assert_eq!(ssh_destination("devbox"), "devbox");
        assert_eq!(ssh_destination("me@devbox"), "me@devbox");
        assert_eq!(ssh_destination("me@devbox:2222"), "ssh://me@devbox:2222");
        assert_eq!(ssh_destination("10.0.0.9:22"), "ssh://10.0.0.9:22");
        assert_eq!(ssh_destination("me@[fd00::9]:2222"), "ssh://me@[fd00::9]:2222");
        assert_eq!(ssh_destination("ssh://me@devbox:2222"), "ssh://me@devbox:2222");
        assert_eq!(ssh_destination("fd00::9"), "fd00::9");
        assert_eq!(ssh_destination("devbox:"), "devbox:");
    }
}
