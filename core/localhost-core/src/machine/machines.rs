//! The other Machines an app shows: which they are, kept in `machines.json`
//! in the storage root, and the connection to each while the app wants it.
//!
//! A connection that is lost is made again on its own, after a pause that
//! grows (`RETRY_DELAYS`), or at once on `wake` (the computer woke, the
//! network changed). That only reconnects; it never starts or stops a
//! Service. A problem that waiting won't fix (an untrusted host key, a
//! sign-in that needs a prompt, no localhost there) waits for the user to
//! connect again.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use super::{Forward, Machine, MachineConnection, MachineProblem, SshLink};
use crate::{Id, LocalhostError};

const FILE_NAME: &str = "machines.json";
const SCHEMA_VERSION: i64 = 1;

/// How long to wait before each new attempt after a connection is lost.
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];

/// Where the app is with one Machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MachineState {
    NotConnected,
    Connecting,
    /// Connected to the localhost of this version there.
    Connected {
        version: String,
    },
    /// Not connected, for this reason. `is_retrying`: another attempt comes
    /// on its own.
    Unavailable {
        problem: MachineProblem,
        is_retrying: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum MachinesEvent {
    /// The list changed: added, removed or renamed.
    MachinesChanged {
        machines: Vec<Machine>,
    },
    StateChanged {
        machine_id: Id,
        state: MachineState,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    schema_version: i64,
    machines: Vec<Machine>,
}

/// Every other Machine, and the connections to them. Cheap to clone. Must
/// be used inside a tokio runtime.
#[derive(Clone)]
pub struct Machines {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    control_folder: PathBuf,
    ssh: PathBuf,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    machines: Vec<Machine>,
    slots: HashMap<Id, Slot>,
    subscribers: Vec<mpsc::UnboundedSender<MachinesEvent>>,
}

struct Slot {
    state: MachineState,
    link: Option<Arc<SshLink>>,
    connection: Option<MachineConnection>,
    keeper: Option<JoinHandle<()>>,
    retry_now: Arc<Notify>,
}

impl Default for Slot {
    fn default() -> Slot {
        Slot {
            state: MachineState::NotConnected,
            link: None,
            connection: None,
            keeper: None,
            retry_now: Arc::new(Notify::new()),
        }
    }
}

impl Machines {
    /// The Machines saved in `storage_root`, reached with the `ssh` at `ssh`.
    pub fn open(storage_root: &Path, ssh: &Path) -> Result<Machines, LocalhostError> {
        let path = storage_root.join(FILE_NAME);
        let machines = match fs::read(&path) {
            Ok(data) => {
                let file: File = serde_json::from_slice(&data).map_err(|error| LocalhostError::PersistenceFailed {
                    path: path.display().to_string(),
                    reason: error.to_string(),
                })?;
                if file.schema_version != SCHEMA_VERSION {
                    return Err(LocalhostError::UnsupportedSchemaVersion {
                        version: file.schema_version,
                        path: path.display().to_string(),
                    });
                }
                file.machines
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(LocalhostError::PersistenceFailed {
                    path: path.display().to_string(),
                    reason: error.to_string(),
                });
            }
        };
        Ok(Machines {
            inner: Arc::new(Inner {
                path,
                control_folder: storage_root.join("machines"),
                ssh: ssh.to_path_buf(),
                state: Mutex::new(State { machines, ..State::default() }),
            }),
        })
    }

    pub fn machines(&self) -> Vec<Machine> {
        self.inner.state().machines.clone()
    }

    /// The Machines, their states, and every change after them.
    pub fn subscribe(&self) -> (Vec<Machine>, HashMap<Id, MachineState>, mpsc::UnboundedReceiver<MachinesEvent>) {
        let mut state = self.inner.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        state.subscribers.push(sender);
        let states = state.machines.iter().map(|machine| (machine.id, state.machine_state(machine.id))).collect();
        (state.machines.clone(), states, receiver)
    }

    pub fn state(&self, machine_id: Id) -> MachineState {
        self.inner.state().machine_state(machine_id)
    }

    /// Remembers the Machine `ssh` reaches as `destination`, called `name`
    /// (by default its destination). Doesn't connect.
    pub fn add(&self, name: Option<String>, destination: &str) -> Result<Machine, LocalhostError> {
        let destination = destination.trim();
        // Passed after `--`, but an option-like destination is never meant.
        if destination.is_empty() || destination.starts_with('-') || destination.chars().any(char::is_whitespace) {
            return Err(LocalhostError::InvalidMachineDestination(destination.to_string()));
        }
        let name = name.map(|name| name.trim().to_string()).filter(|name| !name.is_empty());
        let machine = Machine {
            id: Id::new(),
            name: name.unwrap_or_else(|| destination.to_string()),
            destination: destination.into(),
        };
        let mut state = self.inner.state();
        if state.machines.iter().any(|existing| existing.destination == machine.destination) {
            return Err(LocalhostError::DuplicateMachine(machine.destination));
        }
        let mut machines = state.machines.clone();
        machines.push(machine.clone());
        self.inner.save(&machines)?;
        state.machines = machines.clone();
        state.publish(MachinesEvent::MachinesChanged { machines });
        Ok(machine)
    }

    /// Disconnects and forgets the Machine. Nothing changes on it.
    pub fn remove(&self, machine_id: Id) -> Result<(), LocalhostError> {
        self.disconnect(machine_id);
        let mut state = self.inner.state();
        let machines: Vec<Machine> =
            state.machines.iter().filter(|machine| machine.id != machine_id).cloned().collect();
        self.inner.save(&machines)?;
        state.machines = machines.clone();
        state.slots.remove(&machine_id);
        state.publish(MachinesEvent::MachinesChanged { machines });
        Ok(())
    }

    /// Connects to the Machine, and keeps connecting when the connection is
    /// lost, until `disconnect`. Returns at once; the state says how it goes.
    pub fn connect(&self, machine_id: Id) -> Result<(), LocalhostError> {
        let mut state = self.inner.state();
        let machine = state.machine(machine_id)?;
        let slot = state.slots.entry(machine_id).or_default();
        if slot.keeper.as_ref().is_some_and(|keeper| !keeper.is_finished()) {
            slot.retry_now.notify_one();
            return Ok(());
        }
        let retry_now = slot.retry_now.clone();
        let weak = Arc::downgrade(&self.inner);
        slot.keeper = Some(tokio::spawn(keep_connected(weak, machine, retry_now)));
        Ok(())
    }

    /// Ends the connection to the Machine, and its Forwards. Its Runs are
    /// left to its own rules: an app open there keeps them.
    pub fn disconnect(&self, machine_id: Id) {
        let link = {
            let mut state = self.inner.state();
            let Some(slot) = state.slots.get_mut(&machine_id) else { return };
            if let Some(keeper) = slot.keeper.take() {
                keeper.abort();
            }
            slot.connection = None;
            let link = slot.link.take();
            state.set(machine_id, MachineState::NotConnected);
            link
        };
        if let Some(link) = link {
            link.end();
        }
    }

    /// Tries every connection that is waiting to retry now: the computer
    /// woke up, or its network changed.
    pub fn wake(&self) {
        for slot in self.inner.state().slots.values() {
            slot.retry_now.notify_one();
        }
    }

    /// The connection to the Machine, if connected.
    pub fn connection(&self, machine_id: Id) -> Result<MachineConnection, LocalhostError> {
        let state = self.inner.state();
        let machine = state.machine(machine_id)?;
        match state.slots.get(&machine_id).and_then(|slot| slot.connection.clone()) {
            Some(connection) if connection.is_connected() => Ok(connection),
            _ => Err(LocalhostError::MachineUnavailable { machine: machine.name, problem: MachineProblem::Lost }),
        }
    }

    /// Forwards the Machine's `port` to `local_port` here (`SshLink::forward_to`).
    pub async fn forward(&self, machine_id: Id, port: u16, local_port: u16) -> Result<Forward, LocalhostError> {
        self.link(machine_id)?.forward_to(port, local_port).await
    }

    pub async fn cancel_forward(&self, machine_id: Id, forward: Forward) {
        if let Ok(link) = self.link(machine_id) {
            link.cancel_forward(forward).await;
        }
    }

    pub fn forwards(&self, machine_id: Id) -> Vec<Forward> {
        self.link(machine_id).map(|link| link.forwards()).unwrap_or_default()
    }

    fn link(&self, machine_id: Id) -> Result<Arc<SshLink>, LocalhostError> {
        let state = self.inner.state();
        let machine = state.machine(machine_id)?;
        state
            .slots
            .get(&machine_id)
            .and_then(|slot| slot.link.clone())
            .ok_or(LocalhostError::MachineUnavailable { machine: machine.name, problem: MachineProblem::Lost })
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Writes the list atomically: a reader sees the old file or the new one.
    fn save(&self, machines: &[Machine]) -> Result<(), LocalhostError> {
        let failure = |error: std::io::Error| LocalhostError::PersistenceFailed {
            path: self.path.display().to_string(),
            reason: error.to_string(),
        };
        let file = File { schema_version: SCHEMA_VERSION, machines: machines.to_vec() };
        let mut data = serde_json::to_vec_pretty(&file).expect("Machines always encode");
        data.push(b'\n');
        let folder = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(folder).map_err(failure)?;
        let temporary = folder.join(format!(".{FILE_NAME}.tmp-{}", Id::new()));
        let result = (|| {
            let mut handle = fs::File::create(&temporary)?;
            handle.write_all(&data)?;
            handle.sync_all()?;
            fs::rename(&temporary, &self.path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(failure(error));
        }
        Ok(())
    }
}

impl State {
    fn machine(&self, machine_id: Id) -> Result<Machine, LocalhostError> {
        self.machines
            .iter()
            .find(|machine| machine.id == machine_id)
            .cloned()
            .ok_or(LocalhostError::MachineNotFound(machine_id))
    }

    fn machine_state(&self, machine_id: Id) -> MachineState {
        self.slots.get(&machine_id).map_or(MachineState::NotConnected, |slot| slot.state.clone())
    }

    fn set(&mut self, machine_id: Id, machine_state: MachineState) {
        let slot = self.slots.entry(machine_id).or_default();
        if slot.state == machine_state {
            return;
        }
        slot.state = machine_state.clone();
        self.publish(MachinesEvent::StateChanged { machine_id, state: machine_state });
    }

    fn publish(&mut self, event: MachinesEvent) {
        self.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
}

/// Whether waiting could fix `problem`, so it is worth trying again unasked.
fn is_passing(problem: &MachineProblem) -> bool {
    matches!(
        problem,
        MachineProblem::Lost | MachineProblem::Unreachable(_) | MachineProblem::HostNotFound | MachineProblem::Other(_)
    )
}

/// Connects to `machine` until told to stop (the task is aborted), again
/// after each loss.
async fn keep_connected(inner: std::sync::Weak<Inner>, machine: Machine, retry_now: Arc<Notify>) {
    let mut attempt = 0;
    loop {
        let Some(strong) = inner.upgrade() else { return };
        strong.state().set(machine.id, MachineState::Connecting);
        let (ssh, control_folder) = (strong.ssh.clone(), strong.control_folder.clone());
        drop(strong);

        let problem = match SshLink::connect(machine.clone(), &ssh, &control_folder).await {
            Ok((link, connection)) => {
                attempt = 0;
                let version = connection.hello().version.clone();
                {
                    let Some(strong) = inner.upgrade() else { return };
                    let mut state = strong.state();
                    let slot = state.slots.entry(machine.id).or_default();
                    slot.link = Some(link);
                    slot.connection = Some(connection.clone());
                    state.set(machine.id, MachineState::Connected { version });
                }
                connection.closed().await;
                let Some(strong) = inner.upgrade() else { return };
                let link = {
                    let mut state = strong.state();
                    let slot = state.slots.entry(machine.id).or_default();
                    slot.connection = None;
                    slot.link.take()
                };
                if let Some(link) = link {
                    link.end();
                }
                MachineProblem::Lost
            }
            Err(LocalhostError::MachineUnavailable { problem, .. }) => problem,
            Err(other) => MachineProblem::Other(other.message()),
        };

        let is_retrying = is_passing(&problem);
        if let Some(strong) = inner.upgrade() {
            strong.state().set(machine.id, MachineState::Unavailable { problem, is_retrying });
        }
        if !is_retrying {
            return;
        }
        let delay = RETRY_DELAYS[attempt.min(RETRY_DELAYS.len() - 1)];
        attempt += 1;
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = retry_now.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remembers_machines_across_launches() {
        let root = tempfile::tempdir().unwrap();
        let machines = Machines::open(root.path(), Path::new("ssh")).unwrap();
        let devbox = machines.add(None, " devbox ").unwrap();
        assert_eq!((devbox.name.as_str(), devbox.destination.as_str()), ("devbox", "devbox"));
        let lab = machines.add(Some("Lab".into()), "me@lab.example.com").unwrap();

        let reopened = Machines::open(root.path(), Path::new("ssh")).unwrap();
        assert_eq!(reopened.machines(), [devbox.clone(), lab.clone()]);
        assert_eq!(reopened.state(devbox.id), MachineState::NotConnected);

        reopened.remove(devbox.id).unwrap();
        assert_eq!(Machines::open(root.path(), Path::new("ssh")).unwrap().machines(), [lab]);
    }

    #[tokio::test]
    async fn refuses_destinations_ssh_would_misread() {
        let root = tempfile::tempdir().unwrap();
        let machines = Machines::open(root.path(), Path::new("ssh")).unwrap();
        for destination in ["", "  ", "-oProxyCommand=evil", "dev box"] {
            assert!(matches!(machines.add(None, destination), Err(LocalhostError::InvalidMachineDestination(_))));
        }
        machines.add(None, "devbox").unwrap();
        assert_eq!(
            machines.add(Some("Again".into()), "devbox"),
            Err(LocalhostError::DuplicateMachine("devbox".into()))
        );
    }

    #[tokio::test]
    async fn says_why_it_cant_connect_and_waits_when_waiting_wont_help() {
        let root = tempfile::tempdir().unwrap();
        // An `ssh` that fails as an unknown host key does.
        let ssh = root.path().join("ssh");
        fs::write(&ssh, "#!/bin/sh\necho 'Host key verification failed.' >&2\nexit 255\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let machines = Machines::open(root.path(), &ssh).unwrap();
        let devbox = machines.add(None, "devbox").unwrap();
        let (_, _, mut events) = machines.subscribe();
        machines.connect(devbox.id).unwrap();
        let expected = MachineState::Unavailable { problem: MachineProblem::HostKeyNotTrusted, is_retrying: false };
        tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(event) = events.recv().await {
                if event == (MachinesEvent::StateChanged { machine_id: devbox.id, state: expected.clone() }) {
                    return;
                }
            }
        })
        .await
        .expect("never said why");
        assert!(machines.connection(devbox.id).is_err());
    }
}
