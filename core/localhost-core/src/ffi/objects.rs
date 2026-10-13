//! The objects the app holds. Their methods mirror the crate's own API,
//! reshaped where UniFFI can't carry it: owned arguments instead of borrows,
//! records instead of tuples, and work moved onto the core's runtime so the
//! caller's thread never blocks (a Keychain call can show a prompt).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use super::runtime::{blocking, on_runtime};
use super::subscriptions::{LibraryEvents, OutputLines, ServiceEvents, ServiceOutputLines};
use crate::{
    EnvironmentProfile, Id, Key, KeyClash, Listener, LocalhostError, LogStore, NewService, Origin, Orphan, OutputLine,
    ProfileChanges, Project, ProjectChanges, ProjectDocument, ProjectLibrary, RunRegistry, Service, ServiceChanges,
    ServiceManager, ServiceManagerOptions, ServiceOutputLine, ServiceStatus, StopPolicy, SystemSecretStore,
    UnreadableProjectFile, VariableInput, VariableLayer,
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
    #[uniffi::constructor(name = "open")]
    pub fn ffi_open(storage_root: String) -> Result<Arc<ProjectLibrary>, LocalhostError> {
        ProjectLibrary::open(Path::new(&storage_root), Arc::new(SystemSecretStore::new())).map(Arc::new)
    }

    #[uniffi::method(name = "unreadable_files")]
    pub fn ffi_unreadable_files(&self) -> Vec<UnreadableProjectFile> {
        self.unreadable_files().to_vec()
    }

    #[uniffi::method(name = "projects")]
    pub fn ffi_projects(&self) -> Vec<Project> {
        self.projects()
    }

    #[uniffi::method(name = "project")]
    pub fn ffi_project(&self, id: Id) -> Result<Project, LocalhostError> {
        self.project(id)
    }

    #[uniffi::method(name = "secret_value")]
    pub async fn ffi_secret_value(
        self: Arc<Self>,
        name: String,
        layer: VariableLayer,
    ) -> Result<Option<String>, LocalhostError> {
        blocking(move || self.secret_value(&name, layer)).await
    }

    #[uniffi::method(name = "is_secret_set")]
    pub async fn ffi_is_secret_set(
        self: Arc<Self>,
        name: String,
        layer: VariableLayer,
    ) -> Result<bool, LocalhostError> {
        blocking(move || self.is_secret_set(&name, layer)).await
    }

    #[uniffi::method(name = "subscribe")]
    pub fn ffi_subscribe(&self) -> LibrarySubscription {
        let (projects, receiver) = self.subscribe();
        LibrarySubscription { projects, events: LibraryEvents::new(receiver) }
    }

    #[uniffi::method(name = "events")]
    pub fn ffi_events(&self) -> Arc<LibraryEvents> {
        LibraryEvents::new(self.events())
    }

    #[uniffi::method(name = "add_project")]
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

    #[uniffi::method(name = "edit_project")]
    pub async fn ffi_edit_project(
        self: Arc<Self>,
        id: Id,
        changes: ProjectChanges,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        blocking(move || self.edit_project(id, changes, origin)).await
    }

    #[uniffi::method(name = "remove_project")]
    pub async fn ffi_remove_project(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_project(id, origin)).await
    }

    #[uniffi::method(name = "import_project")]
    pub async fn ffi_import_project(
        self: Arc<Self>,
        document: Arc<ProjectDocument>,
        on_key_clash: Option<KeyClash>,
        origin: Origin,
    ) -> Result<Project, LocalhostError> {
        blocking(move || self.import_project(&document, on_key_clash, origin)).await
    }

    #[uniffi::method(name = "free_project_key")]
    pub fn ffi_free_project_key(&self, key: Key) -> Key {
        self.free_project_key(&key)
    }

    #[uniffi::method(name = "add_service")]
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

    #[uniffi::method(name = "add_services")]
    pub async fn ffi_add_services(
        self: Arc<Self>,
        services: Vec<NewService>,
        project_id: Id,
        origin: Origin,
    ) -> Result<Vec<Service>, LocalhostError> {
        blocking(move || self.add_services(services, project_id, origin)).await
    }

    #[uniffi::method(name = "edit_service")]
    pub async fn ffi_edit_service(
        self: Arc<Self>,
        id: Id,
        changes: ServiceChanges,
        origin: Origin,
    ) -> Result<Service, LocalhostError> {
        blocking(move || self.edit_service(id, changes, origin)).await
    }

    #[uniffi::method(name = "remove_service")]
    pub async fn ffi_remove_service(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_service(id, origin)).await
    }

    #[uniffi::method(name = "add_profile")]
    pub async fn ffi_add_profile(
        self: Arc<Self>,
        project_id: Id,
        name: String,
        variables: Vec<VariableInput>,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        blocking(move || self.add_profile(project_id, &name, variables, origin)).await
    }

    #[uniffi::method(name = "edit_profile")]
    pub async fn ffi_edit_profile(
        self: Arc<Self>,
        id: Id,
        changes: ProfileChanges,
        origin: Origin,
    ) -> Result<EnvironmentProfile, LocalhostError> {
        blocking(move || self.edit_profile(id, changes, origin)).await
    }

    #[uniffi::method(name = "remove_profile")]
    pub async fn ffi_remove_profile(self: Arc<Self>, id: Id, origin: Origin) -> Result<(), LocalhostError> {
        blocking(move || self.remove_profile(id, origin)).await
    }

    #[uniffi::method(name = "set_active_profile")]
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
    #[uniffi::constructor(name = "new")]
    pub fn ffi_new(root: String) -> Arc<LogStore> {
        Arc::new(LogStore::new(Path::new(&root)))
    }

    #[uniffi::method(name = "lines")]
    pub fn ffi_lines(&self, run_id: Id) -> Vec<OutputLine> {
        self.lines(run_id)
    }

    #[uniffi::method(name = "recent_lines")]
    pub fn ffi_recent_lines(&self, run_id: Id, limit: u64) -> Vec<OutputLine> {
        self.recent_lines(run_id, saturating_usize(limit))
    }

    #[uniffi::method(name = "recent_output")]
    pub fn ffi_recent_output(&self, run_id: Id, limit: u64) -> Vec<OutputLine> {
        self.recent_output(run_id, saturating_usize(limit))
    }

    #[uniffi::method(name = "latest_run_id")]
    pub fn ffi_latest_run_id(&self, service_id: Id) -> Option<Id> {
        self.latest_run_id(service_id)
    }

    #[uniffi::method(name = "subscribe_to_run")]
    pub fn ffi_subscribe_to_run(&self, run_id: Id) -> RunSubscription {
        let (lines, receiver) = self.subscribe_to_run(run_id);
        RunSubscription { lines, updates: OutputLines::new(receiver) }
    }

    #[uniffi::method(name = "subscribe_to_services")]
    pub fn ffi_subscribe_to_services(&self, service_ids: Vec<Id>) -> ServicesSubscription {
        let (lines, receiver) = self.subscribe_to_services(service_ids.into_iter().collect::<HashSet<Id>>());
        ServicesSubscription { lines, updates: ServiceOutputLines::new(receiver) }
    }

    #[uniffi::method(name = "clear_buffer")]
    pub fn ffi_clear_buffer(&self, service_id: Id) {
        self.clear_buffer(service_id)
    }

    #[uniffi::method(name = "log_file_path")]
    pub fn ffi_log_file_path(&self, run_id: Id) -> Option<String> {
        self.log_file_path(run_id).map(|path| path.to_string_lossy().into_owned())
    }

    #[uniffi::method(name = "remove_logs")]
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
    #[uniffi::constructor(name = "new")]
    pub fn ffi_new(root: String) -> Arc<RunRegistry> {
        Arc::new(RunRegistry::new(Path::new(&root)))
    }

    #[uniffi::method(name = "orphans")]
    pub fn ffi_orphans(&self) -> Vec<Orphan> {
        self.orphans()
    }

    /// Stops each Orphan's process group with the default stop policy.
    #[uniffi::method(name = "stop_orphans")]
    pub async fn ffi_stop_orphans(self: Arc<Self>, orphans: Vec<Orphan>) {
        on_runtime(async move { self.stop_orphans(&orphans, StopPolicy::default()).await }).await
    }

    #[uniffi::method(name = "leave_orphans")]
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
    #[uniffi::constructor(name = "new")]
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

    #[uniffi::method(name = "status")]
    pub fn ffi_status(&self, service_id: Id) -> ServiceStatus {
        self.status(service_id)
    }

    /// Every known status (Services that never ran are left out), and the
    /// events after it.
    #[uniffi::method(name = "subscribe")]
    pub fn ffi_subscribe(&self) -> ServiceSubscription {
        let (snapshot, receiver) = self.subscribe();
        ServiceSubscription { snapshot: snapshot.into_values().collect(), events: ServiceEvents::new(receiver) }
    }

    #[uniffi::method(name = "start")]
    pub async fn ffi_start(
        self: Arc<Self>,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.start(service_id, on_port, origin).await }).await
    }

    #[uniffi::method(name = "stop")]
    pub async fn ffi_stop(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.stop(service_id, origin).await }).await
    }

    #[uniffi::method(name = "restart")]
    pub async fn ffi_restart(self: Arc<Self>, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        on_runtime(async move { self.restart(service_id, origin).await }).await
    }

    #[uniffi::method(name = "start_all")]
    pub async fn ffi_start_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.start_all(project_id, origin).await }).await
    }

    #[uniffi::method(name = "stop_all")]
    pub async fn ffi_stop_all(self: Arc<Self>, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_all(project_id, origin).await }).await
    }

    #[uniffi::method(name = "stop_every_run")]
    pub async fn ffi_stop_every_run(self: Arc<Self>, origin: Origin) {
        on_runtime(async move { self.stop_every_run(origin).await }).await
    }

    #[uniffi::method(name = "set_automatic_port_move")]
    pub fn ffi_set_automatic_port_move(&self, is_enabled: bool) {
        self.set_automatic_port_move(is_enabled)
    }

    #[uniffi::method(name = "listeners")]
    pub async fn ffi_listeners(self: Arc<Self>) -> Vec<Listener> {
        on_runtime(async move { self.listeners().await }).await
    }

    #[uniffi::method(name = "stop_listener")]
    pub async fn ffi_stop_listener(self: Arc<Self>, listener: Listener, origin: Origin) -> Result<(), LocalhostError> {
        on_runtime(async move { self.stop_listener(&listener, origin).await }).await
    }

    #[uniffi::method(name = "next_free_port")]
    pub async fn ffi_next_free_port(self: Arc<Self>, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        on_runtime(async move { self.next_free_port(service_id).await }).await
    }
}

// MARK: ProjectDocument

#[uniffi::export]
impl ProjectDocument {
    /// The document exporting `project`.
    #[uniffi::constructor(name = "from_project")]
    pub fn ffi_from_project(project: Project) -> Arc<ProjectDocument> {
        Arc::new(ProjectDocument::from(&project))
    }

    #[uniffi::constructor(name = "from_yaml")]
    pub fn ffi_from_yaml(text: String) -> Result<Arc<ProjectDocument>, LocalhostError> {
        ProjectDocument::from_yaml(&text).map(Arc::new)
    }

    /// A copy whose Project lives at `path`. The document itself is
    /// immutable on this side of the bridge.
    #[uniffi::method(name = "with_path")]
    pub fn ffi_with_path(&self, path: String) -> Arc<ProjectDocument> {
        let mut document = self.clone();
        document.set_path(path);
        Arc::new(document)
    }

    #[uniffi::method(name = "name")]
    pub fn ffi_name(&self) -> String {
        self.name().to_string()
    }

    #[uniffi::method(name = "key")]
    pub fn ffi_key(&self) -> Key {
        self.key().clone()
    }

    #[uniffi::method(name = "path")]
    pub fn ffi_path(&self) -> String {
        self.path().to_string()
    }

    #[uniffi::method(name = "variables")]
    pub fn ffi_variables(&self) -> crate::Variables {
        self.variables().clone()
    }

    #[uniffi::method(name = "services")]
    pub fn ffi_services(&self) -> Vec<crate::DocumentService> {
        self.services().to_vec()
    }

    #[uniffi::method(name = "profiles")]
    pub fn ffi_profiles(&self) -> Vec<crate::DocumentProfile> {
        self.profiles().to_vec()
    }

    #[uniffi::method(name = "active_profile")]
    pub fn ffi_active_profile(&self) -> Option<String> {
        self.active_profile().map(str::to_string)
    }

    #[uniffi::method(name = "to_yaml")]
    pub fn ffi_to_yaml(&self) -> String {
        self.to_yaml()
    }
}
