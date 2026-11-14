//! The other Machines an app shows: which they are, kept in `machines.json`
//! in the storage root, the connection to each while the app wants it, and
//! what each shows through it: its Projects and every Service's status.
//!
//! That view outlives a connection. A connection that is lost is made again
//! on its own, after a pause that grows (`RETRY_DELAYS`), or at once on
//! `wake` (the computer woke, the network changed); the Projects and
//! statuses are then read afresh. That only reconnects; it never starts or
//! stops a Service. A problem that waiting won't fix (an untrusted host key,
//! a sign-in that needs a prompt, no localhost there, anything SSH says that
//! localhost doesn't know) is shown once and waits for the user to connect
//! again.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use super::{Forward, Machine, MachineConnection, MachineProblem, ProjectFileText, SshLink};
use crate::{
    DirectoryListing, Id, Key, KeyClash, LocalhostError, NewService, OutputLine, ProfileInput, Project, ProjectChanges,
    Service, ServiceChanges, ServiceEvent, ServiceOutputLine, ServiceStatus, ServiceSuggestion, VariableInput,
    VariableLayer,
};

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
    /// Connected to the localhost of this version there. Without
    /// `manages_projects`, it is too old for its Projects to be changed from
    /// here: they are shown and run only.
    #[serde(rename_all = "camelCase")]
    Connected {
        version: String,
        #[serde(default)]
        manages_projects: bool,
    },
    /// Not connected, for this reason. `is_retrying`: another attempt comes
    /// on its own.
    #[serde(rename_all = "camelCase")]
    Unavailable {
        problem: MachineProblem,
        is_retrying: bool,
    },
}

impl MachineState {
    /// How it is, as the apps say it under its name: "Connected · localhost
    /// 0.5.0", "Connecting…", or why it isn't and what to do next.
    pub fn summary(&self, machine: &str) -> String {
        match self {
            MachineState::NotConnected => "Not connected".into(),
            MachineState::Connecting => "Connecting…".into(),
            MachineState::Connected { version, .. } => format!("Connected · localhost {version}"),
            MachineState::Unavailable { problem, is_retrying: true } => {
                format!("{} Trying again…", problem.message(machine))
            }
            MachineState::Unavailable { problem, is_retrying: false } => problem.message(machine),
        }
    }

    /// Whether it waits for the user to connect it again.
    pub fn needs_user(&self) -> bool {
        matches!(self, MachineState::NotConnected | MachineState::Unavailable { is_retrying: false, .. })
    }
}

/// One Machine as an app shows it. Its Projects and statuses are those of
/// its last connection, kept while it reconnects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MachineSnapshot {
    pub machine: Machine,
    pub state: MachineState,
    pub projects: Vec<Project>,
    /// Services that never ran there are left out.
    pub statuses: Vec<ServiceStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
// Per variant, not `rename_all_fields`, which specta doesn't read.
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
// Not boxed: UniFFI carries no `Box`, and events go one at a time.
#[allow(clippy::large_enum_variant)]
pub enum MachinesEvent {
    /// The list changed: added, removed or renamed.
    MachinesChanged { machines: Vec<Machine> },
    #[serde(rename_all = "camelCase")]
    StateChanged { machine_id: Id, state: MachineState },
    /// Its Projects, after it connected or they changed there.
    #[serde(rename_all = "camelCase")]
    ProjectsChanged { machine_id: Id, projects: Vec<Project> },
    /// Every status there, read afresh after it connected.
    #[serde(rename_all = "camelCase")]
    StatusesChanged { machine_id: Id, statuses: Vec<ServiceStatus> },
    #[serde(rename_all = "camelCase")]
    ServiceChanged { machine_id: Id, event: ServiceEvent },
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
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
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
    projects: Vec<Project>,
    statuses: HashMap<Id, ServiceStatus>,
}

impl Default for Slot {
    fn default() -> Slot {
        Slot {
            state: MachineState::NotConnected,
            link: None,
            connection: None,
            keeper: None,
            retry_now: Arc::new(Notify::new()),
            projects: Vec::new(),
            statuses: HashMap::new(),
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

    /// Every Machine as it is now, and every change after that.
    pub fn subscribe(&self) -> (Vec<MachineSnapshot>, mpsc::UnboundedReceiver<MachinesEvent>) {
        let mut state = self.inner.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        state.subscribers.push(sender);
        let snapshots = state.machines.iter().map(|machine| state.snapshot(machine)).collect();
        (snapshots, receiver)
    }

    pub fn state(&self, machine_id: Id) -> MachineState {
        self.inner.state().machine_state(machine_id)
    }

    /// Connects to the Machine `ssh` reaches as `destination`, and only once
    /// that works remembers it, called `name` (by default its destination),
    /// and keeps it connected. Fails with why it can't be reached.
    pub async fn add(&self, name: Option<String>, destination: &str) -> Result<Machine, LocalhostError> {
        let machine = self.inner.state().new_machine(name, destination)?;
        // Named by its destination while adding: that is what was tried.
        let tried = Machine { name: machine.destination.clone(), ..machine.clone() };
        let (link, connection) = SshLink::connect(tried, &self.inner.ssh, &self.inner.control_folder).await?;
        let mut state = self.inner.state();
        if let Err(error) = state.insert(&self.inner, machine.clone()) {
            link.end();
            return Err(error);
        }
        let slot = state.slots.entry(machine.id).or_default();
        let weak = Arc::downgrade(&self.inner);
        slot.keeper =
            Some(tokio::spawn(keep_connected(weak, machine.id, slot.retry_now.clone(), Some((link, connection)))));
        Ok(machine)
    }

    /// Remembers a Machine without connecting to it.
    #[cfg(test)]
    fn remember(&self, name: Option<String>, destination: &str) -> Result<Machine, LocalhostError> {
        let mut state = self.inner.state();
        let machine = state.new_machine(name, destination)?;
        state.insert(&self.inner, machine.clone())?;
        Ok(machine)
    }

    /// Calls the Machine `name`; an empty one is its destination.
    pub fn rename(&self, machine_id: Id, name: &str) -> Result<Machine, LocalhostError> {
        let mut state = self.inner.state();
        let mut machine = state.machine(machine_id)?;
        let name = name.trim();
        machine.name = if name.is_empty() { machine.destination.clone() } else { name.to_string() };
        let machines: Vec<Machine> = state
            .machines
            .iter()
            .map(|existing| if existing.id == machine_id { machine.clone() } else { existing.clone() })
            .collect();
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
    /// While it is waiting to try again, it tries now.
    pub fn connect(&self, machine_id: Id) -> Result<(), LocalhostError> {
        let mut state = self.inner.state();
        state.machine(machine_id)?;
        let slot = state.slots.entry(machine_id).or_default();
        if slot.keeper.as_ref().is_some_and(|keeper| !keeper.is_finished()) {
            slot.retry_now.notify_one();
            return Ok(());
        }
        let retry_now = slot.retry_now.clone();
        let weak = Arc::downgrade(&self.inner);
        slot.keeper = Some(tokio::spawn(keep_connected(weak, machine_id, retry_now, None)));
        Ok(())
    }

    /// Ends the connection, even a working one, and makes it again.
    pub fn reconnect(&self, machine_id: Id) -> Result<(), LocalhostError> {
        self.inner.state().machine(machine_id)?;
        self.disconnect(machine_id);
        self.connect(machine_id)
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
            _ => Err(LocalhostError::MachineUnavailable { machine: machine.name, problem: state.problem(machine_id) }),
        }
    }

    // MARK: Its Services

    pub async fn start(
        &self,
        machine_id: Id,
        service_id: Id,
        on_port: Option<u16>,
    ) -> Result<ServiceStatus, LocalhostError> {
        self.connection(machine_id)?.start(service_id, on_port).await
    }

    pub async fn stop(&self, machine_id: Id, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        self.connection(machine_id)?.stop(service_id).await
    }

    pub async fn restart(&self, machine_id: Id, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        self.connection(machine_id)?.restart(service_id).await
    }

    pub async fn start_all(&self, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.start_all(project_id).await
    }

    pub async fn stop_all(&self, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.stop_all(project_id).await
    }

    /// A Run's buffered lines there, then every line after them until it ends.
    pub async fn subscribe_to_run(
        &self,
        machine_id: Id,
        run_id: Id,
    ) -> Result<(Vec<OutputLine>, mpsc::UnboundedReceiver<OutputLine>), LocalhostError> {
        self.connection(machine_id)?.subscribe_to_run(run_id).await
    }

    /// Several Services' lines there, buffered then live, as one stream.
    pub async fn subscribe_to_services(
        &self,
        machine_id: Id,
        service_ids: HashSet<Id>,
    ) -> Result<(Vec<ServiceOutputLine>, mpsc::UnboundedReceiver<ServiceOutputLine>), LocalhostError> {
        self.connection(machine_id)?.subscribe_to_services(service_ids).await
    }

    // MARK: Its Project Configuration
    //
    // Made there, as `Origin::Machine`; what changed comes back with its
    // Projects. A Machine with an older localhost refuses with
    // `MachineNeedsUpdate`.

    /// Its Projects, as last read: for a Key no other Project there has.
    pub fn projects(&self, machine_id: Id) -> Result<Vec<Project>, LocalhostError> {
        let state = self.inner.state();
        state.machine(machine_id)?;
        Ok(state.slots.get(&machine_id).map(|slot| slot.projects.clone()).unwrap_or_default())
    }

    /// A folder there, for a folder picker; `None` is the home folder there.
    pub async fn list_directory(
        &self,
        machine_id: Id,
        path: Option<String>,
        show_hidden: bool,
        include_files: bool,
    ) -> Result<DirectoryListing, LocalhostError> {
        self.connection(machine_id)?.list_directory(path, show_hidden, include_files).await
    }

    /// Suggestions for a folder there, scanned there.
    pub async fn scan_folder(&self, machine_id: Id, path: String) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        self.connection(machine_id)?.scan_folder(path).await
    }

    pub async fn scan_project(&self, machine_id: Id, project_id: Id) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        self.connection(machine_id)?.scan_project(project_id).await
    }

    pub async fn detect_python_environment(
        &self,
        machine_id: Id,
        project_id: Id,
        working_directory: String,
        command_line: String,
    ) -> Result<Option<String>, LocalhostError> {
        self.connection(machine_id)?.detect_python_environment(project_id, working_directory, command_line).await
    }

    pub async fn add_project(
        &self,
        machine_id: Id,
        name: String,
        key: Key,
        path: String,
        services: Vec<NewService>,
    ) -> Result<Project, LocalhostError> {
        self.connection(machine_id)?.add_project(name, key, path, services).await
    }

    pub async fn edit_project(
        &self,
        machine_id: Id,
        project_id: Id,
        changes: ProjectChanges,
    ) -> Result<Project, LocalhostError> {
        self.connection(machine_id)?.edit_project(project_id, changes).await
    }

    /// Stops its Services there, then removes it and their logs. Its folder
    /// is not touched.
    pub async fn remove_project(&self, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.remove_project(project_id).await
    }

    pub async fn add_service(
        &self,
        machine_id: Id,
        project_id: Id,
        service: NewService,
        variables: Vec<VariableInput>,
        dependencies: Vec<Id>,
    ) -> Result<Service, LocalhostError> {
        self.connection(machine_id)?.add_service(project_id, service, variables, dependencies).await
    }

    pub async fn add_services(
        &self,
        machine_id: Id,
        project_id: Id,
        services: Vec<NewService>,
    ) -> Result<Vec<Service>, LocalhostError> {
        self.connection(machine_id)?.add_services(project_id, services).await
    }

    pub async fn edit_service(
        &self,
        machine_id: Id,
        service_id: Id,
        changes: ServiceChanges,
    ) -> Result<Service, LocalhostError> {
        self.connection(machine_id)?.edit_service(service_id, changes).await
    }

    /// Stops the Service there, then removes it and its logs.
    pub async fn remove_service(&self, machine_id: Id, service_id: Id) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.remove_service(service_id).await
    }

    pub async fn save_environment(
        &self,
        machine_id: Id,
        project_id: Id,
        variables: Vec<VariableInput>,
        profiles: Vec<ProfileInput>,
        active: Option<u32>,
    ) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.save_environment(project_id, variables, profiles, active).await
    }

    pub async fn set_active_profile(
        &self,
        machine_id: Id,
        project_id: Id,
        profile_id: Option<Id>,
    ) -> Result<(), LocalhostError> {
        self.connection(machine_id)?.set_active_profile(project_id, profile_id).await
    }

    /// A Secret's value there, to reveal it. Never kept here.
    pub async fn secret_value(
        &self,
        machine_id: Id,
        name: String,
        layer: VariableLayer,
    ) -> Result<Option<String>, LocalhostError> {
        self.connection(machine_id)?.secret_value(name, layer).await
    }

    /// A Project file there, for the import's review.
    pub async fn read_project_file(&self, machine_id: Id, path: String) -> Result<ProjectFileText, LocalhostError> {
        self.connection(machine_id)?.read_project_file(path).await
    }

    /// Imports a Project file's `text` into the folder `path` there.
    pub async fn import_project(
        &self,
        machine_id: Id,
        text: String,
        path: String,
        on_key_clash: Option<KeyClash>,
    ) -> Result<Project, LocalhostError> {
        self.connection(machine_id)?.import_project(text, path, on_key_clash).await
    }

    /// The URL to open for a Service of the Machine on this computer, as
    /// for a Service here (`Service::url_to_open`), its Port forwarded to the
    /// same port here first. `None` when it has nothing to open. A URL that
    /// isn't local to the Machine is returned as it is.
    pub async fn open_service(&self, machine_id: Id, service_id: Id) -> Result<Option<String>, LocalhostError> {
        let url = {
            let state = self.inner.state();
            let slot = state.slots.get(&machine_id).ok_or(LocalhostError::MachineNotFound(machine_id))?;
            let service = slot
                .projects
                .iter()
                .flat_map(|project| &project.services)
                .find(|service| service.id == service_id)
                .ok_or(LocalhostError::ServiceNotFound(service_id))?;
            service.url_to_open(slot.statuses.get(&service_id).and_then(|status| status.run.as_ref()))
        };
        let Some(url) = url else { return Ok(None) };
        if let Some(port) = local_port(&url) {
            self.forward(machine_id, port, port).await?;
        }
        Ok(Some(url))
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
            .ok_or(LocalhostError::MachineUnavailable { machine: machine.name, problem: state.problem(machine_id) })
    }
}

/// The port of `url` when its host is the Machine's own (`localhost`, a
/// loopback address, or a `*.localhost` name), which a Forward reaches.
fn local_port(url: &str) -> Option<u16> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed.split_once(']')?;
            (host, after.strip_prefix(':'))
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let host = host.to_ascii_lowercase();
    let is_local =
        matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "0.0.0.0") || host.ends_with(".localhost");
    if !is_local {
        return None;
    }
    match port {
        Some(port) => port.parse().ok(),
        None if scheme.eq_ignore_ascii_case("https") => Some(443),
        None => Some(80),
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

    /// A Machine to add, if `destination` is one `ssh` reads as meant and no
    /// other Machine has.
    fn new_machine(&self, name: Option<String>, destination: &str) -> Result<Machine, LocalhostError> {
        let destination = destination.trim();
        // Passed after `--`, but an option-like destination is never meant.
        if destination.is_empty() || destination.starts_with('-') || destination.chars().any(char::is_whitespace) {
            return Err(LocalhostError::InvalidMachineDestination(destination.to_string()));
        }
        if self.machines.iter().any(|existing| existing.destination == destination) {
            return Err(LocalhostError::DuplicateMachine(destination.to_string()));
        }
        let name = name.map(|name| name.trim().to_string()).filter(|name| !name.is_empty());
        Ok(Machine {
            id: Id::new(),
            name: name.unwrap_or_else(|| destination.to_string()),
            destination: destination.into(),
        })
    }

    fn insert(&mut self, inner: &Inner, machine: Machine) -> Result<(), LocalhostError> {
        if self.machines.iter().any(|existing| existing.destination == machine.destination) {
            return Err(LocalhostError::DuplicateMachine(machine.destination));
        }
        let mut machines = self.machines.clone();
        machines.push(machine);
        inner.save(&machines)?;
        self.machines = machines.clone();
        self.publish(MachinesEvent::MachinesChanged { machines });
        Ok(())
    }

    fn machine_state(&self, machine_id: Id) -> MachineState {
        self.slots.get(&machine_id).map_or(MachineState::NotConnected, |slot| slot.state.clone())
    }

    /// Why the Machine isn't connected, for an error.
    fn problem(&self, machine_id: Id) -> MachineProblem {
        match self.machine_state(machine_id) {
            MachineState::Unavailable { problem, .. } => problem,
            _ => MachineProblem::Lost,
        }
    }

    fn snapshot(&self, machine: &Machine) -> MachineSnapshot {
        let slot = self.slots.get(&machine.id);
        MachineSnapshot {
            machine: machine.clone(),
            state: self.machine_state(machine.id),
            projects: slot.map(|slot| slot.projects.clone()).unwrap_or_default(),
            statuses: slot.map(|slot| slot.statuses.values().cloned().collect()).unwrap_or_default(),
        }
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
/// What SSH says that localhost doesn't recognize is shown once instead: a
/// mistake in `~/.ssh/config` stays one however often it is tried.
fn is_passing(problem: &MachineProblem) -> bool {
    matches!(problem, MachineProblem::Lost | MachineProblem::Unreachable(_) | MachineProblem::HostNotFound)
}

/// Connects to the Machine until told to stop (the task is aborted), again
/// after each loss. `established`: a connection made already, by `add`.
async fn keep_connected(
    inner: Weak<Inner>,
    machine_id: Id,
    retry_now: Arc<Notify>,
    mut established: Option<(Arc<SshLink>, MachineConnection)>,
) {
    let mut attempt = 0;
    loop {
        let connected = match established.take() {
            Some(connected) => Ok(connected),
            None => {
                let Some(strong) = inner.upgrade() else { return };
                let Ok(machine) = strong.state().machine(machine_id) else { return };
                strong.state().set(machine_id, MachineState::Connecting);
                let (ssh, control_folder) = (strong.ssh.clone(), strong.control_folder.clone());
                drop(strong);
                SshLink::connect(machine, &ssh, &control_folder).await
            }
        };
        let problem = match connected {
            Ok((link, connection)) => {
                if follow(&inner, machine_id, link, connection).await {
                    attempt = 0;
                }
                MachineProblem::Lost
            }
            Err(LocalhostError::MachineUnavailable { problem, .. }) => problem,
            Err(other) => MachineProblem::Other(other.message()),
        };

        let is_retrying = is_passing(&problem);
        let Some(strong) = inner.upgrade() else { return };
        strong.state().set(machine_id, MachineState::Unavailable { problem, is_retrying });
        drop(strong);
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

/// Reads the Machine's Projects and statuses over a new connection, then
/// follows their changes until it ends. Returns whether it was connected.
async fn follow(inner: &Weak<Inner>, machine_id: Id, link: Arc<SshLink>, connection: MachineConnection) -> bool {
    {
        let Some(strong) = inner.upgrade() else { return false };
        let mut state = strong.state();
        let slot = state.slots.entry(machine_id).or_default();
        // Only the slot holds the link: once the Machines are gone, the
        // connection ends with it.
        slot.link = Some(link);
        slot.connection = Some(connection.clone());
    }
    let read = async {
        let (projects, project_updates) = connection.projects().await?;
        let (statuses, service_events) = connection.subscribe().await?;
        Ok::<_, LocalhostError>((projects, project_updates, statuses, service_events))
    };
    let is_connected = match read.await {
        Ok((projects, mut project_updates, statuses, mut service_events)) => {
            {
                let Some(strong) = inner.upgrade() else { return false };
                let mut state = strong.state();
                let slot = state.slots.entry(machine_id).or_default();
                slot.projects = projects.clone();
                slot.statuses = statuses;
                let statuses = slot.statuses.values().cloned().collect();
                state.publish(MachinesEvent::ProjectsChanged { machine_id, projects });
                state.publish(MachinesEvent::StatusesChanged { machine_id, statuses });
                let version = connection.hello().version.clone();
                let manages_projects = connection.manages_projects();
                state.set(machine_id, MachineState::Connected { version, manages_projects });
            }
            loop {
                tokio::select! {
                    Some(projects) = project_updates.recv() => {
                        let Some(strong) = inner.upgrade() else { return true };
                        let mut state = strong.state();
                        state.slots.entry(machine_id).or_default().projects = projects.clone();
                        state.publish(MachinesEvent::ProjectsChanged { machine_id, projects });
                    }
                    Some(event) = service_events.recv() => {
                        let Some(strong) = inner.upgrade() else { return true };
                        let mut state = strong.state();
                        apply(&mut state.slots.entry(machine_id).or_default().statuses, &event);
                        state.publish(MachinesEvent::ServiceChanged { machine_id, event });
                    }
                    _ = connection.closed() => break,
                }
            }
            true
        }
        Err(_) => {
            connection.close();
            false
        }
    };
    if let Some(strong) = inner.upgrade() {
        let link = {
            let mut state = strong.state();
            let slot = state.slots.entry(machine_id).or_default();
            slot.connection = None;
            slot.link.take()
        };
        if let Some(link) = link {
            link.end();
        }
    }
    is_connected
}

/// Updates the statuses as the Service event says.
fn apply(statuses: &mut HashMap<Id, ServiceStatus>, event: &ServiceEvent) {
    match event {
        ServiceEvent::ServiceStateChanged(status) => {
            statuses.insert(status.service_id, status.clone());
        }
        ServiceEvent::RunStarted(run) | ServiceEvent::RunEnded(run) => {
            if let Some(status) = statuses.get_mut(&run.service_id)
                && status.run.as_ref().is_some_and(|current| current.id == run.id)
            {
                status.run = Some(run.clone());
            }
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
        let devbox = machines.remember(None, " devbox ").unwrap();
        assert_eq!((devbox.name.as_str(), devbox.destination.as_str()), ("devbox", "devbox"));
        let lab = machines.remember(Some("Lab".into()), "me@lab.example.com").unwrap();

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
            assert!(matches!(machines.remember(None, destination), Err(LocalhostError::InvalidMachineDestination(_))));
        }
        machines.remember(None, "devbox").unwrap();
        assert_eq!(
            machines.remember(Some("Again".into()), "devbox"),
            Err(LocalhostError::DuplicateMachine("devbox".into()))
        );
    }

    // The fake `ssh` is a shell script.
    #[cfg(unix)]
    #[tokio::test]
    async fn says_why_it_cant_connect_and_waits_when_waiting_wont_help() {
        let root = tempfile::tempdir().unwrap();
        // An `ssh` that fails as an unknown host key does.
        let ssh = failing_ssh(root.path(), "Host key verification failed.");
        let machines = Machines::open(root.path(), &ssh).unwrap();
        let devbox = machines.remember(None, "devbox").unwrap();
        let (_, mut events) = machines.subscribe();
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

    /// An `ssh` at `root`/ssh that writes `said` to standard error and fails.
    #[cfg(unix)]
    fn failing_ssh(root: &Path, said: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let ssh = root.join("ssh");
        fs::write(&ssh, format!("#!/bin/sh\necho '{said}' >&2\nexit 255\n")).unwrap();
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
        // A test forking on another thread may hold the file open for
        // writing a moment longer, so it can't be run yet ("Text file busy").
        for _ in 0..100 {
            match std::process::Command::new(&ssh).output() {
                Err(error) if error.raw_os_error() == Some(26) => std::thread::sleep(Duration::from_millis(20)),
                _ => break,
            }
        }
        ssh
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn adds_a_machine_only_once_it_is_reached() {
        let root = tempfile::tempdir().unwrap();
        let ssh = failing_ssh(root.path(), "ssh: Could not resolve hostname devbx: Name or service not known");
        let machines = Machines::open(root.path(), &ssh).unwrap();
        let error = machines.add(Some("Dev".into()), "devbx").await.unwrap_err();
        assert_eq!(
            error,
            // Named by the host it tried, not the name it would have had.
            LocalhostError::MachineUnavailable { machine: "devbx".into(), problem: MachineProblem::HostNotFound }
        );
        assert!(machines.machines().is_empty());
        assert!(Machines::open(root.path(), &ssh).unwrap().machines().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shows_what_ssh_says_once_instead_of_retrying_it() {
        let root = tempfile::tempdir().unwrap();
        let ssh = failing_ssh(root.path(), "/home/me/.ssh/config: line 3: Bad configuration option: hostnme");
        let machines = Machines::open(root.path(), &ssh).unwrap();
        let devbox = machines.remember(None, "devbox").unwrap();
        let (_, mut events) = machines.subscribe();
        machines.connect(devbox.id).unwrap();
        let mut attempts = 0;
        let mut last = None;
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = events.recv().await {
                if let MachinesEvent::StateChanged { state, .. } = event {
                    if state == MachineState::Connecting {
                        attempts += 1;
                    }
                    last = Some(state);
                }
            }
        })
        .await;
        assert_eq!(attempts, 1, "tried again what waiting won't fix");
        assert_eq!(
            last,
            Some(MachineState::Unavailable {
                problem: MachineProblem::Other(
                    "/home/me/.ssh/config: line 3: Bad configuration option: hostnme".into()
                ),
                is_retrying: false,
            })
        );
    }

    #[tokio::test]
    async fn renames_a_machine() {
        let root = tempfile::tempdir().unwrap();
        let machines = Machines::open(root.path(), Path::new("ssh")).unwrap();
        let devbox = machines.remember(None, "me@devbox").unwrap();
        let (_, mut events) = machines.subscribe();
        assert_eq!(machines.rename(devbox.id, " Dev box ").unwrap().name, "Dev box");
        assert!(matches!(events.recv().await, Some(MachinesEvent::MachinesChanged { .. })));
        assert_eq!(machines.rename(devbox.id, "  ").unwrap().name, "me@devbox");
        let reopened = Machines::open(root.path(), Path::new("ssh")).unwrap();
        assert_eq!(reopened.machines()[0].name, "me@devbox");
        assert!(matches!(machines.rename(Id::new(), "x"), Err(LocalhostError::MachineNotFound(_))));
    }

    #[test]
    fn says_how_a_machine_is() {
        let lost = MachineState::Unavailable { problem: MachineProblem::Lost, is_retrying: true };
        assert_eq!(lost.summary("devbox"), "The connection to devbox was lost. Trying again…");
        assert!(!lost.needs_user());
        let untrusted = MachineState::Unavailable { problem: MachineProblem::HostKeyNotTrusted, is_retrying: false };
        assert!(untrusted.needs_user());
        assert_eq!(
            MachineState::Connected { version: "0.5.0".into(), manages_projects: true }.summary("devbox"),
            "Connected · localhost 0.5.0"
        );
        assert!(MachineState::NotConnected.needs_user());
    }

    #[test]
    fn forwards_only_urls_local_to_the_machine() {
        assert_eq!(local_port("http://localhost:3000/app"), Some(3000));
        assert_eq!(local_port("http://127.0.0.1:5173"), Some(5173));
        assert_eq!(local_port("http://[::1]:8080/"), Some(8080));
        assert_eq!(local_port("http://shop.localhost:3000"), Some(3000));
        assert_eq!(local_port("https://localhost"), Some(443));
        assert_eq!(local_port("http://LOCALHOST"), Some(80));
        assert_eq!(local_port("https://app.example.com:3000"), None);
        assert_eq!(local_port("not a url"), None);
    }
}
