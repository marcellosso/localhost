//! The objects the app holds. Their methods (`ffi_`-prefixed, so the Swift
//! layer can expose the idiomatic names) mirror the crate's own API,
//! reshaped where UniFFI can't carry it: owned arguments instead of borrows,
//! records instead of tuples, and work moved onto the core's runtime so the
//! caller's thread never blocks (a Keychain call can show a prompt).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::runtime::{blocking, on_runtime};
use super::subscriptions::{
    FoundOrphansEvents, LibraryEvents, MachinesEvents, OutputLines, ServiceEvents, ServiceOutputLines,
};
use crate::machine::{Forward, MachineSnapshot, Machines, ProjectFileText};
use crate::remote::{Client, shared};
use crate::{
    AgentClient, AgentSetup, AgentStatus, CommandLineStatus, DirectoryListing, EnvironmentProfile, ExecutionTarget, Id,
    Key, KeyClash, Link, Listener, LocalhostError, LogStore, Machine, NewService, Origin, Orphan, OutputLine,
    ProfileChanges, ProfileInput, Project, ProjectChanges, ProjectDocument, ProjectLibrary, RunRegistry, RunStats,
    Service, ServiceChanges, ServiceManager, ServiceManagerOptions, ServiceOutputLine, ServiceStatus,
    ServiceSuggestion, StopPolicy, SystemSecretStore, Targets, Telemetry, UnreadableProjectFile, VariableInput,
    VariableLayer,
};

// MARK: ProjectLibrary

#[derive(uniffi::Record)]
pub struct LibrarySubscription {
    pub projects: Vec<Project>,
    pub events: Arc<LibraryEvents>,
}

#[uniffi::export]
impl ProjectLibrary {
    /// Loads every stored Project from `storage_root`, with Secrets in the
    /// system store (the Keychain on macOS).
    #[uniffi::constructor]
    pub fn ffi_open(storage_root: String) -> Result<Arc<ProjectLibrary>, LocalhostError> {
        ProjectLibrary::open(Path::new(&storage_root), Arc::new(SystemSecretStore::new())).map(Arc::new)
    }

    #[uniffi::method]
    pub fn ffi_unreadable_files(&self) -> Vec<UnreadableProjectFile> {
        self.unreadable_files().to_vec()
    }

    #[uniffi::method]
    pub fn ffi_projects(&self) -> Vec<Project> {
        self.projects()
    }

    #[uniffi::method]
    pub fn ffi_project(&self, id: Id) -> Result<Project, LocalhostError> {
        self.project(id)
    }

    #[uniffi::method]
    pub async fn ffi_secret_value(
        self: Arc<Self>,
        name: String,
        layer: VariableLayer,
    ) -> Result<Option<String>, LocalhostError> {
        blocking(move || self.secret_value(&name, layer)).await
    }

    #[uniffi::method]
    pub async fn ffi_is_secret_set(
        self: Arc<Self>,
        name: String,
        layer: VariableLayer,
    ) -> Result<bool, LocalhostError> {
        blocking(move || self.is_secret_set(&name, layer)).await
    }

    #[uniffi::method]
    pub fn ffi_subscribe(&self) -> LibrarySubscription {
        let (projects, receiver) = self.subscribe();
        LibrarySubscription { projects, events: LibraryEvents::new(receiver) }
    }

    #[uniffi::method]
    pub fn ffi_events(&self) -> Arc<LibraryEvents> {
        LibraryEvents::new(self.events())
    }

    #[uniffi::method]
    pub async fn ffi_add_project(
        self: Arc<Self>,
        name: String,
        key: Key,
        path: String,
        services: Vec<NewService>,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        blocking(move || self.add_project(&name, key, &path, services, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_edit_project(
        self: Arc<Self>,
        id: Id,
        changes: ProjectChanges,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        blocking(move || self.edit_project(id, changes, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_remove_project(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_project(id, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_import_project(
        self: Arc<Self>,
        document: Arc<ProjectDocument>,
        on_key_clash: Option<KeyClash>,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        blocking(move || self.import_project(&document, on_key_clash, origin)).await
    }

    #[uniffi::method]
    pub fn ffi_free_project_key(&self, key: Key) -> Key {
        self.free_project_key(&key)
    }

    #[uniffi::method]
    pub async fn ffi_add_service(
        self: Arc<Self>,
        project_id: Id,
        service: NewService,
        variables: Vec<VariableInput>,
        dependencies: Vec<Id>,
        origin: Origin,
    ) -> Result<Service, LocalhostError> {
        blocking(move || self.add_service(project_id, service, variables, dependencies, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_add_services(
        self: Arc<Self>,
        services: Vec<NewService>,
        project_id: Id,
        origin: Origin,
    ) -> Result<Vec<Service>, LocalhostError> {
        blocking(move || self.add_services(services, project_id, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_edit_service(
        self: Arc<Self>,
        id: Id,
        changes: ServiceChanges,
        origin: Origin,
    ) -> Result<Service, LocalhostError> {
        blocking(move || self.edit_service(id, changes, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_remove_service(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_service(id, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_add_profile(
        self: Arc<Self>,
        project_id: Id,
        name: String,
        variables: Vec<VariableInput>,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        blocking(move || self.add_profile(project_id, &name, variables, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_edit_profile(
        self: Arc<Self>,
        id: Id,
        changes: ProfileChanges,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        blocking(move || self.edit_profile(id, changes, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_remove_profile(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_profile(id, origin)).await
    }

    #[uniffi::method]
    pub async fn ffi_set_active_profile(
        self: Arc<Self>,
        profile_id: Option<Id>,
        project_id: Id,
        origin: Origin,
    ) -> Result<(), LocalhostError> {
        blocking(move || self.set_active_profile(profile_id, project_id, origin)).await
    }
}

// MARK: LogStore

#[derive(uniffi::Record)]
pub struct RunSubscription {
    pub lines: Vec<OutputLine>,
    pub updates: Arc<OutputLines>,
}

#[derive(uniffi::Record)]
pub struct ServicesSubscription {
    pub lines: Vec<ServiceOutputLine>,
    pub updates: Arc<ServiceOutputLines>,
}

#[uniffi::export]
impl LogStore {
    /// A log store keeping its files in `root`/logs, with the default limits.
    #[uniffi::constructor]
    pub fn ffi_new(root: String) -> Arc<LogStore> {
        Arc::new(LogStore::new(Path::new(&root)))
    }

    #[uniffi::method]
    pub fn ffi_lines(&self, run_id: Id) -> Vec<OutputLine> {
        self.lines(run_id)
    }

    #[uniffi::method]
    pub fn ffi_recent_lines(&self, run_id: Id, limit: u64) -> Vec<OutputLine> {
        self.recent_lines(run_id, saturating_usize(limit))
    }

    #[uniffi::method]
    pub fn ffi_recent_output(&self, run_id: Id, limit: u64) -> Vec<OutputLine> {
        self.recent_output(run_id, saturating_usize(limit))
    }

    #[uniffi::method]
    pub fn ffi_latest_run_id(&self, service_id: Id) -> Option<Id> {
        self.latest_run_id(service_id)
    }

    #[uniffi::method]
    pub fn ffi_subscribe_to_run(&self, run_id: Id) -> RunSubscription {
        let (lines, receiver) = self.subscribe_to_run(run_id);
        RunSubscription { lines, updates: OutputLines::new(receiver) }
    }

    #[uniffi::method]
    pub fn ffi_subscribe_to_services(&self, service_ids: Vec<Id>) -> ServicesSubscription {
        let (lines, receiver) = self.subscribe_to_services(service_ids.into_iter().collect::<HashSet<Id>>());
        ServicesSubscription { lines, updates: ServiceOutputLines::new(receiver) }
    }

    #[uniffi::method]
    pub fn ffi_clear_buffer(&self, service_id: Id) {
        self.clear_buffer(service_id)
    }

    #[uniffi::method]
    pub fn ffi_log_file_path(&self, run_id: Id) -> Option<String> {
        self.log_file_path(run_id).map(|path| path.to_string_lossy().into_owned())
    }

    #[uniffi::method]
    pub async fn ffi_remove_logs(self: Arc<Self>, service_id: Id) {
        blocking(move || self.remove_logs(service_id)).await
    }
}

/// `LogStore::DEFAULT_MAX_LINES_PER_RUN`: UniFFI doesn't export constants.
#[uniffi::export]
pub fn log_store_default_max_lines_per_run() -> u64 {
    LogStore::DEFAULT_MAX_LINES_PER_RUN as u64
}

fn saturating_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

// MARK: RunRegistry

#[uniffi::export]
impl RunRegistry {
    /// The Run records kept in `root`/runs.
    #[uniffi::constructor]
    pub fn ffi_new(root: String) -> Arc<RunRegistry> {
        Arc::new(RunRegistry::new(Path::new(&root)))
    }

    #[uniffi::method]
    pub fn ffi_orphans(&self) -> Vec<Orphan> {
        self.orphans()
    }

    /// Stops each Orphan's process group with the default stop policy.
    #[uniffi::method]
    pub async fn ffi_stop_orphans(self: Arc<Self>, orphans: Vec<Orphan>) {
        on_runtime(async move { self.stop_orphans(&orphans, StopPolicy::default()).await }).await
    }

    #[uniffi::method]
    pub fn ffi_leave_orphans(&self, orphans: Vec<Orphan>) {
        self.leave_orphans(&orphans)
    }
}

// MARK: ServiceManager

#[derive(uniffi::Record)]
pub struct ServiceSubscription {
    pub snapshot: Vec<ServiceStatus>,
    pub events: Arc<ServiceEvents>,
}

#[uniffi::export]
impl ServiceManager {
    /// A manager with the default options, starting with the automatic Port
    /// move on or off.
    #[uniffi::constructor]
    pub fn ffi_new(
        library: Arc<ProjectLibrary>,
        logs: Arc<LogStore>,
        run_registry: Arc<RunRegistry>,
        automatic_port_move: bool,
    ) -> Arc<ServiceManager> {
        let options = ServiceManagerOptions { automatic_port_move, ..ServiceManagerOptions::default() };
        // It spawns a task watching the library.
        let _runtime = super::runtime::enter();
        Arc::new(ServiceManager::new(library, logs, run_registry, options))
    }

    #[uniffi::method]
    pub fn ffi_status(&self, service_id: Id) -> ServiceStatus {
        self.status(service_id)
    }

    /// Every known status (Services that never ran are left out), and the
    /// events after it.
    #[uniffi::method]
    pub fn ffi_subscribe(&self) -> ServiceSubscription {
        let (snapshot, receiver) = self.subscribe();
        ServiceSubscription { snapshot: snapshot.into_values().collect(), events: ServiceEvents::new(receiver) }
    }

    #[uniffi::method]
    pub async fn ffi_start(
        self: Arc<Self>,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.start(service_id, on_port, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.stop(service_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_restart(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.restart(service_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_start_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.start_all(project_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_all(project_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_every_run(self: Arc<Self>, origin: Origin) {
        on_runtime(async move { self.stop_every_run(origin).await }).await
    }

    #[uniffi::method]
    pub fn ffi_set_automatic_port_move(&self, is_enabled: bool) {
        self.set_automatic_port_move(is_enabled)
    }

    #[uniffi::method]
    pub async fn ffi_listeners(self: Arc<Self>) -> Vec<Listener> {
        on_runtime(async move { self.listeners().await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_listener(self: Arc<Self>, listener: Listener, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_listener(&listener, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_next_free_port(self: Arc<Self>, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        on_runtime(async move { self.next_free_port(service_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_run_stats(
        self: Arc<Self>,
        service_id: Id,
        previous: Option<RunStats>,
    ) -> Result<Option<RunStats>, LocalhostError> {
        on_runtime(async move { self.run_stats(service_id, previous).await }).await
    }
}

// MARK: Targets

/// What the app runs Services through: this machine's shared core, which
/// `localhost mcp` drives too. The app's executable is started as that core
/// (`--daemon <storage root>`) when it isn't running.
#[uniffi::export]
impl Targets {
    #[uniffi::constructor]
    pub fn ffi_open_shared(
        library: Arc<ProjectLibrary>,
        storage_root: String,
        automatic_port_move: bool,
    ) -> Arc<Targets> {
        let root = std::path::PathBuf::from(storage_root);
        let launcher_root = root.clone();
        let launcher = Box::new(move |target: &ExecutionTarget| match target {
            ExecutionTarget::ThisMachine => {
                let executable = std::env::current_exe().unwrap_or_else(|_| "localhost".into());
                let mut daemon = std::process::Command::new(executable);
                daemon.arg("--daemon").arg(&launcher_root);
                shared::link(&launcher_root, Client::App, daemon)
            }
            ExecutionTarget::Wsl { distro } => {
                Link::Unavailable(format!("“{distro}” is reached only from the Windows app"))
            }
        });
        let options = ServiceManagerOptions { automatic_port_move, ..ServiceManagerOptions::default() };
        // It spawns a task watching the library.
        let _runtime = super::runtime::enter();
        Arc::new(Targets::open(library, &root, options, launcher))
    }

    /// Connects to the shared core now, starting it if needed, so the Runs
    /// it already has show at once and its Orphans are reported.
    #[uniffi::method]
    pub async fn ffi_connect(self: Arc<Self>) -> Result<(), LocalhostError> {
        on_runtime(async move { self.connect(&ExecutionTarget::ThisMachine).await }).await
    }

    #[uniffi::method]
    pub fn ffi_status(&self, service_id: Id) -> ServiceStatus {
        self.status(service_id)
    }

    #[uniffi::method]
    pub fn ffi_subscribe(&self) -> ServiceSubscription {
        let (snapshot, receiver) = self.subscribe();
        ServiceSubscription { snapshot: snapshot.into_values().collect(), events: ServiceEvents::new(receiver) }
    }

    #[uniffi::method]
    pub async fn ffi_start(
        self: Arc<Self>,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.start(service_id, on_port, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.stop(service_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_restart(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.restart(service_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_start_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.start_all(project_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_all(project_id, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_every_run(self: Arc<Self>, origin: Origin) {
        on_runtime(async move { self.stop_every_run(origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_retire_shared_core(self: Arc<Self>) {
        on_runtime(async move { self.retire_shared_core().await }).await
    }

    #[uniffi::method]
    pub async fn ffi_connected_agent_count(self: Arc<Self>) -> u64 {
        on_runtime(async move { self.connected_agent_count().await as u64 }).await
    }

    #[uniffi::method]
    pub async fn ffi_disconnect_agents(self: Arc<Self>) -> u64 {
        on_runtime(async move { self.disconnect_agents().await as u64 }).await
    }

    #[uniffi::method]
    pub fn ffi_set_automatic_port_move(&self, is_enabled: bool) {
        self.set_automatic_port_move(is_enabled)
    }

    /// This machine's Listeners: the macOS app has no other target.
    #[uniffi::method]
    pub async fn ffi_listeners(self: Arc<Self>) -> Vec<Listener> {
        let listeners = on_runtime(async move { self.listeners().await }).await;
        listeners.into_iter().map(|(_, listener)| listener).collect()
    }

    #[uniffi::method]
    pub async fn ffi_stop_listener(self: Arc<Self>, listener: Listener, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_listener(&ExecutionTarget::ThisMachine, &listener, origin).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_next_free_port(self: Arc<Self>, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        on_runtime(async move { self.next_free_port(service_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_run_stats(
        self: Arc<Self>,
        service_id: Id,
        previous: Option<RunStats>,
    ) -> Result<Option<RunStats>, LocalhostError> {
        on_runtime(async move { self.run_stats(service_id, previous).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_subscribe_to_run(self: Arc<Self>, run_id: Id) -> Result<RunSubscription, LocalhostError> {
        let (lines, receiver) = on_runtime(async move { self.subscribe_to_run(run_id).await }).await?;
        Ok(RunSubscription { lines, updates: OutputLines::new(receiver) })
    }

    #[uniffi::method]
    pub async fn ffi_subscribe_to_services(
        self: Arc<Self>,
        service_ids: Vec<Id>,
    ) -> Result<ServicesSubscription, LocalhostError> {
        let service_ids = service_ids.into_iter().collect::<HashSet<Id>>();
        let (lines, receiver) = on_runtime(async move { self.subscribe_to_services(service_ids).await }).await?;
        Ok(ServicesSubscription { lines, updates: ServiceOutputLines::new(receiver) })
    }

    #[uniffi::method]
    pub async fn ffi_clear_buffer(self: Arc<Self>, service_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.clear_buffer(service_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_remove_logs(self: Arc<Self>, service_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.remove_logs(&ExecutionTarget::ThisMachine, service_id).await }).await
    }

    /// The Orphans the shared core finds as it connects.
    #[uniffi::method]
    pub fn ffi_found_orphans(&self) -> Arc<FoundOrphansEvents> {
        let mut found = self.found_orphans();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        super::runtime::spawn(async move {
            while let Some(found) = found.recv().await {
                if sender.send(found.orphans).is_err() {
                    return;
                }
            }
        });
        FoundOrphansEvents::new(receiver)
    }

    #[uniffi::method]
    pub async fn ffi_stop_orphans(self: Arc<Self>, orphans: Vec<Orphan>) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_orphans(&ExecutionTarget::ThisMachine, &orphans).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_leave_orphans(self: Arc<Self>, orphans: Vec<Orphan>) -> Result<(), LocalhostError> {
        on_runtime(async move { self.leave_orphans(&ExecutionTarget::ThisMachine, &orphans).await }).await
    }
}

// MARK: Machines

#[derive(uniffi::Record)]
pub struct MachinesSubscription {
    pub machines: Vec<MachineSnapshot>,
    pub events: Arc<MachinesEvents>,
}

/// The other Machines the app shows, reached with the system's `ssh`. One
/// object for all of them: a Machine's controls and logs go through it by
/// the Machine's id, so they survive its reconnections. Every change is made
/// as `Origin::Machine` on the other Machine, which decides it.
#[uniffi::export]
impl Machines {
    /// The Machines saved in `storage_root`.
    #[uniffi::constructor]
    pub fn ffi_open(storage_root: String) -> Result<Arc<Machines>, LocalhostError> {
        Machines::open(Path::new(&storage_root), Path::new("ssh")).map(Arc::new)
    }

    #[uniffi::method]
    pub fn ffi_machines(&self) -> Vec<Machine> {
        self.machines()
    }

    #[uniffi::method]
    pub fn ffi_subscribe(&self) -> MachinesSubscription {
        let (machines, receiver) = self.subscribe();
        MachinesSubscription { machines, events: MachinesEvents::new(receiver) }
    }

    /// Connects first, and remembers the Machine only once that works.
    #[uniffi::method]
    pub async fn ffi_add(
        self: Arc<Self>,
        name: Option<String>,
        destination: String,
    ) -> Result<Machine, LocalhostError> {
        on_runtime(async move { self.add(name, &destination).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_rename(self: Arc<Self>, machine_id: Id, name: String) -> Result<Machine, LocalhostError> {
        blocking(move || self.rename(machine_id, &name)).await
    }

    #[uniffi::method]
    pub async fn ffi_remove(self: Arc<Self>, machine_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.remove(machine_id) }).await
    }

    #[uniffi::method]
    pub fn ffi_connect(&self, machine_id: Id) -> Result<(), LocalhostError> {
        let _runtime = super::runtime::enter();
        self.connect(machine_id)
    }

    #[uniffi::method]
    pub fn ffi_reconnect(&self, machine_id: Id) -> Result<(), LocalhostError> {
        let _runtime = super::runtime::enter();
        self.reconnect(machine_id)
    }

    #[uniffi::method]
    pub fn ffi_disconnect(&self, machine_id: Id) {
        let _runtime = super::runtime::enter();
        self.disconnect(machine_id)
    }

    /// The computer woke or its network changed: tries waiting connections now.
    #[uniffi::method]
    pub fn ffi_wake(&self) {
        self.wake()
    }

    #[uniffi::method]
    pub async fn ffi_start(
        self: Arc<Self>,
        machine_id: Id,
        service_id: Id,
        on_port: Option<u16>,
    ) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.start(machine_id, service_id, on_port).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop(self: Arc<Self>, machine_id: Id, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.stop(machine_id, service_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_restart(self: Arc<Self>, machine_id: Id, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.restart(machine_id, service_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_start_all(self: Arc<Self>, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.start_all(machine_id, project_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_stop_all(self: Arc<Self>, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_all(machine_id, project_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_subscribe_to_run(
        self: Arc<Self>,
        machine_id: Id,
        run_id: Id,
    ) -> Result<RunSubscription, LocalhostError> {
        let (lines, receiver) = on_runtime(async move { self.subscribe_to_run(machine_id, run_id).await }).await?;
        Ok(RunSubscription { lines, updates: OutputLines::new(receiver) })
    }

    #[uniffi::method]
    pub async fn ffi_subscribe_to_services(
        self: Arc<Self>,
        machine_id: Id,
        service_ids: Vec<Id>,
    ) -> Result<ServicesSubscription, LocalhostError> {
        let service_ids = service_ids.into_iter().collect::<HashSet<Id>>();
        let (lines, receiver) =
            on_runtime(async move { self.subscribe_to_services(machine_id, service_ids).await }).await?;
        Ok(ServicesSubscription { lines, updates: ServiceOutputLines::new(receiver) })
    }

    /// The URL to open for the Service, its Port forwarded here first.
    #[uniffi::method]
    pub async fn ffi_open_service(
        self: Arc<Self>,
        machine_id: Id,
        service_id: Id,
    ) -> Result<Option<String>, LocalhostError> {
        on_runtime(async move { self.open_service(machine_id, service_id).await }).await
    }

    #[uniffi::method]
    pub fn ffi_forwards(&self, machine_id: Id) -> Vec<Forward> {
        self.forwards(machine_id)
    }

    // Its Project Configuration, changed there. An older localhost there
    // refuses with `MachineNeedsUpdate`.

    /// A folder there, for the folder picker; `None` is the home folder there.
    #[uniffi::method]
    pub async fn ffi_list_directory(
        self: Arc<Self>,
        machine_id: Id,
        path: Option<String>,
        show_hidden: bool,
        include_files: bool,
    ) -> Result<DirectoryListing, LocalhostError> {
        on_runtime(async move { self.list_directory(machine_id, path, show_hidden, include_files).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_scan_folder(
        self: Arc<Self>,
        machine_id: Id,
        path: String,
    ) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        on_runtime(async move { self.scan_folder(machine_id, path).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_scan_project(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
    ) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        on_runtime(async move { self.scan_project(machine_id, project_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_detect_python_environment(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        working_directory: String,
        command_line: String,
    ) -> Result<Option<String>, LocalhostError> {
        on_runtime(async move {
            self.detect_python_environment(machine_id, project_id, working_directory, command_line).await
        })
        .await
    }

    #[uniffi::method]
    pub async fn ffi_add_project(
        self: Arc<Self>,
        machine_id: Id,
        name: String,
        key: Key,
        path: String,
        services: Vec<NewService>,
    ) -> Result<Project, LocalhostError> {
        on_runtime(async move { self.add_project(machine_id, name, key, path, services).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_edit_project(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        changes: ProjectChanges,
    ) -> Result<Project, LocalhostError> {
        on_runtime(async move { self.edit_project(machine_id, project_id, changes).await }).await
    }

    /// Stops its Services there first, then removes it and their logs.
    #[uniffi::method]
    pub async fn ffi_remove_project(self: Arc<Self>, machine_id: Id, project_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.remove_project(machine_id, project_id).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_add_service(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        service: NewService,
        variables: Vec<VariableInput>,
        dependencies: Vec<Id>,
    ) -> Result<Service, LocalhostError> {
        on_runtime(async move { self.add_service(machine_id, project_id, service, variables, dependencies).await })
            .await
    }

    #[uniffi::method]
    pub async fn ffi_add_services(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        services: Vec<NewService>,
    ) -> Result<Vec<Service>, LocalhostError> {
        on_runtime(async move { self.add_services(machine_id, project_id, services).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_edit_service(
        self: Arc<Self>,
        machine_id: Id,
        service_id: Id,
        changes: ServiceChanges,
    ) -> Result<Service, LocalhostError> {
        on_runtime(async move { self.edit_service(machine_id, service_id, changes).await }).await
    }

    /// Stops the Service there first, then removes it and its logs.
    #[uniffi::method]
    pub async fn ffi_remove_service(self: Arc<Self>, machine_id: Id, service_id: Id) -> Result<(), LocalhostError> {
        on_runtime(async move { self.remove_service(machine_id, service_id).await }).await
    }

    /// The Environment editor: the Project's variables, its Profiles, and
    /// which one is active (an index into `profiles`).
    #[uniffi::method]
    pub async fn ffi_save_environment(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        variables: Vec<VariableInput>,
        profiles: Vec<ProfileInput>,
        active: Option<u32>,
    ) -> Result<(), LocalhostError> {
        on_runtime(async move { self.save_environment(machine_id, project_id, variables, profiles, active).await })
            .await
    }

    #[uniffi::method]
    pub async fn ffi_set_active_profile(
        self: Arc<Self>,
        machine_id: Id,
        project_id: Id,
        profile_id: Option<Id>,
    ) -> Result<(), LocalhostError> {
        on_runtime(async move { self.set_active_profile(machine_id, project_id, profile_id).await }).await
    }

    /// A Secret's value there, to reveal it. Never stored here.
    #[uniffi::method]
    pub async fn ffi_secret_value(
        self: Arc<Self>,
        machine_id: Id,
        name: String,
        layer: VariableLayer,
    ) -> Result<Option<String>, LocalhostError> {
        on_runtime(async move { self.secret_value(machine_id, name, layer).await }).await
    }

    #[uniffi::method]
    pub async fn ffi_read_project_file(
        self: Arc<Self>,
        machine_id: Id,
        path: String,
    ) -> Result<ProjectFileText, LocalhostError> {
        on_runtime(async move { self.read_project_file(machine_id, path).await }).await
    }

    /// Imports a Project file's `text` into the folder `path` there.
    #[uniffi::method]
    pub async fn ffi_import_project(
        self: Arc<Self>,
        machine_id: Id,
        text: String,
        path: String,
        on_key_clash: Option<KeyClash>,
    ) -> Result<Project, LocalhostError> {
        on_runtime(async move { self.import_project(machine_id, text, path, on_key_clash).await }).await
    }
}

// MARK: ProjectDocument

#[uniffi::export]
impl ProjectDocument {
    /// The document exporting `project`.
    #[uniffi::constructor]
    pub fn ffi_from_project(project: Project) -> Arc<ProjectDocument> {
        Arc::new(ProjectDocument::from(&project))
    }

    #[uniffi::constructor]
    pub fn ffi_from_yaml(text: String) -> Result<Arc<ProjectDocument>, LocalhostError> {
        ProjectDocument::from_yaml(&text).map(Arc::new)
    }

    /// A copy whose Project lives at `path`. The document itself is
    /// immutable on this side of the bridge.
    #[uniffi::method]
    pub fn ffi_with_path(&self, path: String) -> Arc<ProjectDocument> {
        let mut document = self.clone();
        document.set_path(path);
        Arc::new(document)
    }

    #[uniffi::method]
    pub fn ffi_name(&self) -> String {
        self.name().to_string()
    }

    #[uniffi::method]
    pub fn ffi_key(&self) -> Key {
        self.key().clone()
    }

    #[uniffi::method]
    pub fn ffi_path(&self) -> String {
        self.path().to_string()
    }

    #[uniffi::method]
    pub fn ffi_variables(&self) -> crate::Variables {
        self.variables().clone()
    }

    #[uniffi::method]
    pub fn ffi_services(&self) -> Vec<crate::DocumentService> {
        self.services().to_vec()
    }

    #[uniffi::method]
    pub fn ffi_profiles(&self) -> Vec<crate::DocumentProfile> {
        self.profiles().to_vec()
    }

    #[uniffi::method]
    pub fn ffi_active_profile(&self) -> Option<String> {
        self.active_profile().map(str::to_string)
    }

    #[uniffi::method]
    pub fn ffi_to_yaml(&self) -> String {
        self.to_yaml()
    }
}

// MARK: AgentSetup

#[uniffi::export]
impl AgentSetup {
    /// Agents of this user, pointed at `command` (the app's bundled `localhost`).
    #[uniffi::constructor]
    pub fn ffi_new(command: String, storage_root: String) -> Arc<AgentSetup> {
        Arc::new(AgentSetup::new(command, Path::new(&storage_root)))
    }

    #[uniffi::method]
    pub fn ffi_command(&self) -> String {
        self.command().to_string_lossy().into_owned()
    }

    #[uniffi::method]
    pub async fn ffi_statuses(self: Arc<Self>) -> Vec<AgentStatus> {
        blocking(move || self.statuses()).await
    }

    #[uniffi::method]
    pub async fn ffi_connect(self: Arc<Self>, client: AgentClient) -> Result<AgentStatus, LocalhostError> {
        blocking(move || self.connect(client)).await
    }

    #[uniffi::method]
    pub async fn ffi_disconnect(self: Arc<Self>, client: AgentClient) -> Result<AgentStatus, LocalhostError> {
        blocking(move || self.disconnect(client)).await
    }

    #[uniffi::method]
    pub async fn ffi_command_line(self: Arc<Self>) -> CommandLineStatus {
        blocking(move || self.command_line()).await
    }

    #[uniffi::method]
    pub async fn ffi_add_command_to_path(self: Arc<Self>) -> Result<CommandLineStatus, LocalhostError> {
        blocking(move || self.add_command_to_path()).await
    }

    #[uniffi::method]
    pub async fn ffi_remove_command_from_path(self: Arc<Self>) -> Result<CommandLineStatus, LocalhostError> {
        blocking(move || self.remove_command_from_path()).await
    }
}

// MARK: Telemetry

#[uniffi::export]
impl Telemetry {
    /// The app's anonymous usage telemetry, its state kept in `storage_root`.
    #[uniffi::constructor]
    pub fn ffi_open(storage_root: String) -> Arc<Telemetry> {
        Arc::new(Telemetry::open(Path::new(&storage_root)))
    }

    #[uniffi::method]
    pub fn ffi_is_enabled(&self) -> bool {
        self.is_enabled()
    }

    #[uniffi::method]
    pub fn ffi_set_enabled(&self, enabled: bool) -> Result<(), LocalhostError> {
        self.set_enabled(enabled)
    }

    #[uniffi::method]
    pub fn ffi_is_notice_pending(&self) -> bool {
        self.is_notice_pending()
    }

    #[uniffi::method]
    pub fn ffi_acknowledge_notice(&self) {
        self.acknowledge_notice()
    }

    /// Returns at once; the event is sent on a thread of its own.
    #[uniffi::method]
    pub fn ffi_record_app_open(self: Arc<Self>) {
        self.record_app_open()
    }
}
