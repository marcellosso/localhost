//! Runs Services: owns each Service's state and its at most one active Run.
//!
//! Start, Stop and Restart are safe to call in any state and from anywhere:
//! operations on one Service run one at a time, in call order, and each
//! returns the Service's status once it has taken effect.
//!
//! State lives behind a mutex that is only ever held for short synchronous
//! sections, never across an `.await`: like the Swift actor it replaces,
//! state may change while an operation waits.

mod run;

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, SystemTime};

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;

pub use run::{PortChange, PortChangeReason, Run, ServiceEvent, ServiceState, ServiceStatus};

use crate::ansi::Ansi;
use crate::listener::{self, Listener, ListenerKind, ListenerOwner};
use crate::local_url::LocalUrl;
use crate::logs::LogStore;
use crate::output::{OutputChannel, OutputLine};
use crate::platform::{self, ProcessStartTime};
use crate::process::{LaunchConfiguration, ProcessTree, ProcessTreeExit, StopPolicy, Termination};
use crate::run_registry::RunRegistry;
use crate::run_stats::{self, RunStats};
use crate::{Id, Key, LocalhostError, Origin, Project, ProjectLibrary, ProjectLibraryEvent, RunEnvironment, Service};

/// What Docker's CLI prints when its daemon isn't running: older versions,
/// then current ones (`failed to connect to the docker API at unix://…;
/// check if the path is correct and if the daemon is running`).
const DOCKER_DAEMON_ERROR_TEXTS: [&str; 2] =
    ["Cannot connect to the Docker daemon", "failed to connect to the docker API"];

/// What `cmd.exe` prints to stderr for a command it can't find. `cmd /c`
/// then exits with 1, so the message is the only sign (English Windows only).
const COMMAND_NOT_RECOGNIZED_TEXT: &str = "is not recognized as an internal or external command";

/// The failure a Run's output line diagnoses, if any.
fn diagnosis(line: &OutputLine) -> Option<LocalhostError> {
    if DOCKER_DAEMON_ERROR_TEXTS.iter().any(|text| line.text.contains(text)) {
        return Some(LocalhostError::DockerDaemonUnavailable);
    }
    let is_not_recognized =
        cfg!(windows) && line.channel == OutputChannel::Stderr && line.text.contains(COMMAND_NOT_RECOGNIZED_TEXT);
    is_not_recognized.then_some(LocalhostError::CommandNotFound)
}

/// How a ServiceManager runs Services.
#[derive(Clone, Debug)]
pub struct ServiceManagerOptions {
    /// The environment every Run starts from (see `RunEnvironment`). On
    /// Windows, variables put on top of the user's environment, which is read
    /// fresh at every start (`RunEnvironment::base`).
    pub base_environment: BTreeMap<String, String>,
    /// How long a Run without a Port must stay alive in Starting to be Running.
    pub grace_period: Duration,
    /// How long a Run with a Port waits in Starting for the Port to accept
    /// connections before it is Running anyway, with a warning.
    pub port_timeout: Duration,
    /// How often the Port is checked meanwhile.
    pub port_probe_interval: Duration,
    pub stop_policy: StopPolicy,
    /// Whether a Run may move to a free port when its Service's Port is taken.
    pub automatic_port_move: bool,
}

impl Default for ServiceManagerOptions {
    fn default() -> Self {
        ServiceManagerOptions {
            base_environment: if cfg!(windows) { BTreeMap::new() } else { RunEnvironment::current() },
            grace_period: Duration::from_millis(1500),
            port_timeout: Duration::from_secs(30),
            port_probe_interval: Duration::from_millis(250),
            stop_policy: StopPolicy::default(),
            automatic_port_move: true,
        }
    }
}

/// Runs Services. Cheap to clone; every clone is the same manager. Must be
/// created inside a multi-threaded tokio runtime.
#[derive(Clone)]
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct ServiceManager {
    inner: Arc<Inner>,
}

struct Inner {
    library: Arc<ProjectLibrary>,
    logs: Arc<LogStore>,
    run_registry: Arc<RunRegistry>,
    options: ServiceManagerOptions,
    automatic_port_move: AtomicBool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    statuses: HashMap<Id, ServiceStatus>,
    /// The process tree of each Service's active Run. It outlives the
    /// Service's Starting/Running states while leftovers are being cleaned up.
    trees: HashMap<Id, ProcessTree>,
    /// The completion of each Service's last queued operation.
    operation_tails: HashMap<Id, oneshot::Receiver<()>>,
    subscribers: Vec<mpsc::UnboundedSender<ServiceEvent>>,
    /// Services that are Starting but still waiting for their Dependencies,
    /// with a token identifying that start request.
    pending_starts: HashMap<Id, Id>,
    /// Waiting for the next status update of a Service.
    update_waiters: HashMap<Id, Vec<oneshot::Sender<()>>>,
    /// The port of each Service's active Run, from launch until its tree has ended.
    run_ports: HashMap<Id, u16>,
    /// The launch configuration each Service's latest Run was started with,
    /// hashed (it includes Secret values, so it is never stored or sent).
    launch_fingerprints: HashMap<Id, u64>,
    /// Live Runs that printed Docker's "daemon not running" error.
    /// Failures diagnosed from a Run's output (`diagnosis`), the first per Run.
    diagnosed_failures: HashMap<Id, LocalhostError>,
}

#[derive(Clone)]
enum DependencyOutcome {
    Satisfied,
    Failed(Key),
    Stopped,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl ServiceManager {
    pub fn new(
        library: Arc<ProjectLibrary>,
        logs: Arc<LogStore>,
        run_registry: Arc<RunRegistry>,
        options: ServiceManagerOptions,
    ) -> ServiceManager {
        let mut events = library.events();
        let inner = Arc::new(Inner {
            library,
            logs,
            run_registry,
            automatic_port_move: AtomicBool::new(options.automatic_port_move),
            options,
            state: Mutex::new(State::default()),
        });
        // Marks live Runs whose configuration changed. Weak, so the manager
        // can be dropped while the library lives on.
        let weak: Weak<Inner> = Arc::downgrade(&inner);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let Some(inner) = weak.upgrade() else { return };
                if let ProjectLibraryEvent::ProjectUpdated { project, .. } = event {
                    inner.configuration_changed(&project).await;
                }
            }
        });
        ServiceManager { inner }
    }

    // MARK: Observing

    pub fn status(&self, service_id: Id) -> ServiceStatus {
        self.inner.status(service_id)
    }

    /// The status of every Service that has run, plus a stream of every
    /// change after that snapshot. Taking both at once means no change falls
    /// between them.
    pub fn subscribe(&self) -> (HashMap<Id, ServiceStatus>, mpsc::UnboundedReceiver<ServiceEvent>) {
        let mut state = self.inner.lock();
        let (sender, receiver) = mpsc::unbounded_channel();
        state.subscribers.push(sender);
        (state.statuses.clone(), receiver)
    }

    // MARK: Lifecycle

    /// Starts a Run unless one is Starting or Running. If the Service is
    /// Stopping, waits for that to finish first. A Run that cannot launch
    /// leaves the Service Failed, with the reason in `run.failure`.
    ///
    /// Dependencies that are not Running or Exited are started first, and the
    /// Service stays Starting until they are. If one Fails, the Service Fails
    /// with `DependencyFailed`; if one is stopped meanwhile, it ends Stopped.
    ///
    /// A Service with a Port whose Port is taken moves to the next free port
    /// when allowed (see `next_free_port`), else Fails with `PortConflict`.
    ///
    /// `on_port`: start on this port instead of the Service's Port, for this
    /// Run only. Ignored for a Service without a Port.
    pub async fn start(
        &self,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        let (project, service) = self.inner.library.service(service_id)?;
        if on_port == Some(0) {
            return Err(LocalhostError::InvalidPort(0));
        }
        let inner = &self.inner;
        if service.dependencies.is_empty() {
            let operation = { inner.clone() }.perform_start(service_id, on_port, origin);
            return Ok(inner.serialized(service_id, operation).await);
        }
        inner.clone().start_with_dependencies(service_id, project, on_port, origin).await;
        Ok(inner.status(service_id))
    }

    /// Stops the active Run, if any, and returns once its whole process tree
    /// has ended. A requested stop always ends Stopped, including a start
    /// still waiting for its Dependencies.
    pub async fn stop(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        self.inner.library.service(service_id)?;
        if self.inner.lock_and(|inner, state| inner.cancel_pending_start(state, service_id)) {
            return Ok(self.inner.status(service_id));
        }
        let operation = self.inner.clone().perform_stop(service_id, origin);
        Ok(self.inner.serialized(service_id, operation).await)
    }

    /// Stops the active Run, if any, then starts a new one (with its
    /// Dependencies, like `start`). Dependents are left alone.
    pub async fn restart(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        let (_, service) = self.inner.library.service(service_id)?;
        let is_pending = self.inner.lock().pending_starts.contains_key(&service_id);
        if service.dependencies.is_empty() && !is_pending {
            let inner = self.inner.clone();
            let operation = async move {
                inner.clone().perform_stop(service_id, origin).await;
                inner.perform_start(service_id, None, origin).await
            };
            return Ok(self.inner.serialized(service_id, operation).await);
        }
        self.stop(service_id, origin).await?;
        self.start(service_id, None, origin).await
    }

    /// Starts every Service of the Project except those excluded from Start
    /// All, each after its Dependencies. Returns once every start has taken
    /// effect (launched, Failed, or cancelled).
    pub async fn start_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let project = self.inner.library.project(project_id)?;
        let service_ids: Vec<Id> =
            project.services.iter().filter(|service| !service.is_excluded_from_start_all).map(|s| s.id).collect();
        // Mark every Service Starting before any of them checks its
        // Dependencies, so none mistakes an earlier Run's Exited for this one.
        let begun: Vec<(Id, Id)> = self.inner.lock_and(|inner, state| {
            service_ids.iter().filter_map(|&id| inner.begin_pending_start(state, id).map(|token| (id, token))).collect()
        });
        let mut starts = JoinSet::new();
        for &(service_id, token) in &begun {
            let operation = self.inner.clone().complete_pending_start(service_id, token, project.clone(), None, origin);
            starts.spawn(operation);
        }
        for &service_id in &service_ids {
            if !begun.iter().any(|(id, _)| *id == service_id) {
                starts.spawn(self.inner.clone().start_with_dependencies(service_id, project.clone(), None, origin));
            }
        }
        starts.join_all().await;
        Ok(())
    }

    /// Stops every Service of the Project, each after the Services that
    /// depend on it. Starts still waiting for their Dependencies are cancelled.
    pub async fn stop_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let project = self.inner.library.project(project_id)?;
        self.inner.lock_and(|inner, state| {
            for service in &project.services {
                inner.cancel_pending_start(state, service.id);
            }
        });
        let all: Vec<Id> = project.services.iter().map(|service| service.id).collect();
        let mut stopped: HashMap<Id, watch::Receiver<bool>> = HashMap::new();
        let mut stops = JoinSet::new();
        for service_id in project.dependency_order(&all).into_iter().rev() {
            let dependents: Vec<watch::Receiver<bool>> = project
                .dependents(service_id)
                .iter()
                .filter_map(|dependent| stopped.get(&dependent.id).cloned())
                .collect();
            let (done, receiver) = watch::channel(false);
            stopped.insert(service_id, receiver);
            let inner = self.inner.clone();
            stops.spawn(async move {
                for mut dependent in dependents {
                    let _ = dependent.wait_for(|done| *done).await;
                }
                let operation = inner.clone().perform_stop(service_id, origin);
                inner.serialized(service_id, operation).await;
                done.send_replace(true);
            });
        }
        stops.join_all().await;
        Ok(())
    }

    /// Stops every live process tree in parallel, e.g. before quitting.
    /// Returns once all of them have ended.
    pub async fn stop_every_run(&self, origin: Origin) {
        let service_ids: Vec<Id> = self.inner.lock_and(|inner, state| {
            let pending: Vec<Id> = state.pending_starts.keys().copied().collect();
            for service_id in pending {
                inner.cancel_pending_start(state, service_id);
            }
            state.trees.keys().copied().collect()
        });
        let mut stops = JoinSet::new();
        for service_id in service_ids {
            let operation = self.inner.clone().perform_stop_for_quit(service_id, origin);
            stops.spawn(self.inner.serialized(service_id, operation));
        }
        stops.join_all().await;
    }

    // MARK: Listeners

    /// Turns automatic port moves on or off (an app setting, on by default).
    pub fn set_automatic_port_move(&self, is_enabled: bool) {
        self.inner.automatic_port_move.store(is_enabled, Ordering::Relaxed);
    }

    /// Every Listener on this machine, by first port, attributed to the Run
    /// whose process tree it is in, else to the Project whose folder holds
    /// its working directory (the deepest one), else to no one.
    pub async fn listeners(&self) -> Vec<Listener> {
        self.inner.listeners().await
    }

    /// Stops a Listener and returns once it has ended. One that belongs to a
    /// Run stops that Service. Any other gets SIGINT → SIGTERM → SIGKILL on
    /// its process group (or, if the group is led by a session leader such as
    /// a terminal's shell, on its process and descendants). localhost itself,
    /// launchd jobs and Docker's backend are refused.
    pub async fn stop_listener(&self, listener: &Listener, origin: Origin) -> Result<(), LocalhostError> {
        if let ListenerOwner::Run { service_id, run_id } = listener.owner {
            if self.inner.status(service_id).run.is_some_and(|run| run.id == run_id) {
                self.stop(service_id, origin).await?;
            }
            return Ok(());
        }
        if let Some(reason) = listener.reason_not_stoppable() {
            return Err(reason);
        }
        listener::stop_listener(listener, self.inner.options.stop_policy).await;
        Ok(())
    }

    // MARK: Stats

    /// What the Service's active Run uses now: its processes, memory and,
    /// given the previous sample of the same Run, its CPU use since then.
    /// `None` if the Service has no active Run. Nothing is sampled between
    /// calls: a client asks again (every second or two) while it shows them.
    pub async fn run_stats(
        &self,
        service_id: Id,
        previous: Option<RunStats>,
    ) -> Result<Option<RunStats>, LocalhostError> {
        self.inner.library.service(service_id)?;
        let active = self.inner.lock_and(|_, state| {
            let run = state.statuses.get(&service_id)?.run.as_ref().filter(|run| run.ended_at.is_none())?;
            let tree = state.trees.get(&service_id).filter(|tree| tree.exit().is_none())?;
            Some((run.id, tree.clone()))
        });
        let Some((run_id, tree)) = active else { return Ok(None) };
        let (usage, sampled_at) = tokio::task::spawn_blocking(move || (tree.usage(), run_stats::sampling_clock()))
            .await
            .expect("sampling a process tree panicked");
        Ok(Some(RunStats::new(run_id, usage, sampled_at, previous.as_ref())))
    }

    /// The port a start of the Service would move to if its Port were taken:
    /// the first free port above it, up to +100, that is not another
    /// Service's Port. `None` if the Service has no Port or none is free.
    pub async fn next_free_port(&self, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        let (_, service) = self.inner.library.service(service_id)?;
        let Some(port) = service.port else { return Ok(None) };
        let listeners = self.inner.listeners().await;
        Ok(self.inner.next_free_port(port, service_id, &listeners).await)
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_and<T>(&self, body: impl FnOnce(&Inner, &mut State) -> T) -> T {
        body(self, &mut self.lock())
    }

    fn status(&self, service_id: Id) -> ServiceStatus {
        status_in(&self.lock(), service_id)
    }

    /// Runs `operation` after the Service's previously queued ones. The
    /// place in the queue is taken when this is called, not when the result
    /// is awaited; the operation runs to completion even if the caller stops
    /// waiting.
    fn serialized<T: Send + 'static>(
        &self,
        service_id: Id,
        operation: impl Future<Output = T> + Send + 'static,
    ) -> impl Future<Output = T> + Send + 'static {
        let (done, tail) = oneshot::channel();
        let previous = self.lock().operation_tails.insert(service_id, tail);
        let task = tokio::spawn(async move {
            if let Some(previous) = previous {
                let _ = previous.await;
            }
            let result = operation.await;
            let _ = done.send(());
            result
        });
        async move { task.await.expect("a Service operation panicked") }
    }

    /// Waits while `condition` holds for the Service's status. The check and
    /// the wait are registered together, so no update falls between them.
    async fn wait_while(&self, service_id: Id, condition: impl Fn(&ServiceStatus) -> bool) {
        loop {
            let update = {
                let mut state = self.lock();
                if !condition(&status_in(&state, service_id)) {
                    return;
                }
                next_update(&mut state, service_id)
            };
            let _ = update.await;
        }
    }

    // MARK: Listeners

    async fn listeners(&self) -> Vec<Listener> {
        let (processes, jobs) = tokio::task::spawn_blocking(listener::scan).await.unwrap_or_default();
        // The kernel reports working directories with symlinks resolved.
        let project_folders: Vec<(Id, String)> = self
            .library
            .projects()
            .into_iter()
            .map(|project| (project.id, platform::resolved_path(&project.path)))
            .collect();
        let runs: Vec<(ProcessTree, Id, Id)> = self.lock_and(|_, state| {
            state
                .trees
                .iter()
                .filter_map(|(service_id, tree)| {
                    let run_id = state.statuses.get(service_id)?.run.as_ref()?.id;
                    Some((tree.clone(), *service_id, run_id))
                })
                .collect()
        });
        let mut listeners: Vec<Listener> = processes
            .into_iter()
            .map(|process| {
                let run = runs.iter().find(|(tree, _, _)| tree.contains(&process));
                let owner = if let Some(&(_, service_id, run_id)) = run {
                    ListenerOwner::Run { service_id, run_id }
                } else if let Some((project_id, _)) = process.working_directory.as_deref().and_then(|directory| {
                    project_folders
                        .iter()
                        .filter(|(_, folder)| platform::is_inside(directory, folder))
                        .max_by_key(|(_, folder)| folder.len())
                }) {
                    ListenerOwner::Project { project_id: *project_id }
                } else {
                    ListenerOwner::Unmanaged
                };
                let launchd_label = jobs.get(&process.pid).or_else(|| jobs.get(&process.process_group_id)).cloned();
                let kind = ListenerKind::of(process.executable_path.as_deref(), launchd_label.as_deref(), owner);
                Listener {
                    pid: process.pid,
                    process_group_id: process.process_group_id,
                    name: process.name,
                    command: process.command,
                    executable_path: process.executable_path,
                    ports: process.ports,
                    working_directory: process.working_directory,
                    owner,
                    launchd_label,
                    kind,
                    start_time: process.start_time,
                }
            })
            .collect();
        listeners.sort_by_key(|listener| (listener.ports.first().copied().unwrap_or(0), listener.pid));
        listeners
    }

    /// The port a Run of `service` gets, and why it is not the Port if it isn't.
    async fn port_for_run(
        &self,
        service: &Service,
        configured_port: u16,
        project: &Project,
        requested_port: Option<u16>,
    ) -> Result<(u16, Option<PortChange>), LocalhostError> {
        let listeners = self.listeners().await;
        if let Some(requested) = requested_port {
            let (is_taken, holder) = self.holder(requested, service.id, &listeners).await;
            if is_taken {
                return Err(LocalhostError::PortConflict { port: requested, owner: holder.map(Box::new) });
            }
            let change = (requested != configured_port)
                .then_some(PortChange { configured_port, reason: PortChangeReason::Requested });
            return Ok((requested, change));
        }
        let (is_taken, holder) = self.holder(configured_port, service.id, &listeners).await;
        if !is_taken {
            return Ok((configured_port, None));
        }
        let conflict = || LocalhostError::PortConflict { port: configured_port, owner: holder.clone().map(Box::new) };
        if !self.can_move_automatically(service, project, holder.as_ref()) {
            return Err(conflict());
        }
        let Some(free_port) = self.next_free_port(configured_port, service.id, &listeners).await else {
            return Err(conflict());
        };
        let reason = PortChangeReason::PortInUse { holder: holder.clone().map(Box::new) };
        Ok((free_port, Some(PortChange { configured_port, reason })))
    }

    /// Whether `port` is taken, and by which Listener if localhost can see it.
    async fn holder(&self, port: u16, service_id: Id, listeners: &[Listener]) -> (bool, Option<Listener>) {
        if let Some(listener) = listeners.iter().find(|listener| listener.ports.contains(&port)) {
            return (true, Some(listener.clone()));
        }
        // Another Run launched on it that isn't listening yet.
        let launched = self.lock().run_ports.iter().any(|(id, run_port)| *id != service_id && *run_port == port);
        if launched {
            return (true, None);
        }
        (crate::port_probe::is_accepting(port).await, None)
    }

    /// Never when it is off, for this Service or app-wide; when dependents may
    /// expect the Port (#15); or when the holder is inside the same Project,
    /// likely its own leftover.
    fn can_move_automatically(&self, service: &Service, project: &Project, holder: Option<&Listener>) -> bool {
        if !self.automatic_port_move.load(Ordering::Relaxed)
            || !service.port_can_change
            || !project.dependents(service.id).is_empty()
        {
            return false;
        }
        match holder.map(|holder| holder.owner) {
            Some(ListenerOwner::Project { project_id }) => project_id != project.id,
            Some(ListenerOwner::Run { service_id, .. }) => project.service(service_id).is_none(),
            Some(ListenerOwner::Unmanaged) | None => true,
        }
    }

    async fn next_free_port(&self, port: u16, service_id: Id, listeners: &[Listener]) -> Option<u16> {
        if port == u16::MAX {
            return None;
        }
        let mut taken: HashSet<u16> = self
            .library
            .projects()
            .iter()
            .flat_map(|project| &project.services)
            .filter(|service| service.id != service_id)
            .filter_map(|service| service.port)
            .collect();
        taken.extend(listeners.iter().flat_map(|listener| listener.ports.iter().copied()));
        taken.extend(self.lock().run_ports.iter().filter(|(id, _)| **id != service_id).map(|(_, port)| *port));
        let last = port.saturating_add(100);
        for candidate in port + 1..=last {
            if !taken.contains(&candidate) && !crate::port_probe::is_accepting(candidate).await {
                return Some(candidate);
            }
        }
        None
    }

    // MARK: Dependencies

    /// `start` for a Service with Dependencies.
    fn start_with_dependencies(
        self: Arc<Self>,
        service_id: Id,
        project: Project,
        requested_port: Option<u16>,
        origin: Origin,
    ) -> BoxFuture<()> {
        Box::pin(async move {
            self.wait_while(service_id, |status| status.state == ServiceState::Stopping).await;
            let Some(token) = self.lock_and(|inner, state| inner.begin_pending_start(state, service_id)) else {
                return;
            };
            self.complete_pending_start(service_id, token, project, requested_port, origin).await;
        })
    }

    /// Marks the Service Starting, waiting for its Dependencies. Returns
    /// `None` if it is already Starting, Running or Stopping.
    fn begin_pending_start(&self, state: &mut State, service_id: Id) -> Option<Id> {
        let mut status = status_in(state, service_id);
        if status.state.is_active() || state.pending_starts.contains_key(&service_id) {
            return None;
        }
        let token = Id::new();
        state.pending_starts.insert(service_id, token);
        status.state = ServiceState::Starting;
        update(state, status);
        Some(token)
    }

    /// Waits for the Dependencies of a pending start, then launches its Run
    /// unless the start was cancelled meanwhile.
    fn complete_pending_start(
        self: Arc<Self>,
        service_id: Id,
        token: Id,
        project: Project,
        requested_port: Option<u16>,
        origin: Origin,
    ) -> BoxFuture<()> {
        Box::pin(async move {
            let dependencies = project.service(service_id).map(|s| s.dependencies.clone()).unwrap_or_default();
            self.lock_and(|_, state| {
                if state.pending_starts.get(&service_id) == Some(&token) {
                    let mut status = status_in(state, service_id);
                    status.awaited_dependencies = dependencies.clone();
                    update(state, status);
                }
            });
            let mut waits = JoinSet::new();
            for dependency in dependencies {
                let (inner, project) = (self.clone(), project.clone());
                waits.spawn(async move {
                    let outcome = inner.clone().await_dependency(dependency, project, origin).await;
                    inner.lock_and(|_, state| {
                        if state.pending_starts.get(&service_id) == Some(&token) {
                            let mut status = status_in(state, service_id);
                            status.awaited_dependencies.retain(|id| *id != dependency);
                            update(state, status);
                        }
                    });
                    outcome
                });
            }
            let outcomes = waits.join_all().await;

            let failed = outcomes.iter().find_map(|outcome| match outcome {
                DependencyOutcome::Failed(key) => Some(key.clone()),
                _ => None,
            });
            let stopped = outcomes.iter().any(|outcome| matches!(outcome, DependencyOutcome::Stopped));
            let should_launch = self.lock_and(|inner, state| {
                if state.pending_starts.get(&service_id) != Some(&token) {
                    return false;
                }
                if let Some(key) = failed {
                    state.pending_starts.remove(&service_id);
                    record_failed_launch(state, service_id, origin, LocalhostError::DependencyFailed(key));
                    return false;
                }
                if stopped {
                    inner.cancel_pending_start(state, service_id);
                    return false;
                }
                true
            });
            if should_launch {
                let operation = self.clone().launch_pending_start(service_id, token, requested_port, origin);
                self.serialized(service_id, operation).await;
            }
        })
    }

    async fn launch_pending_start(
        self: Arc<Self>,
        service_id: Id,
        token: Id,
        requested_port: Option<u16>,
        origin: Origin,
    ) -> ServiceStatus {
        // Leftovers of the previous Run end while the Service is still pending.
        self.wait_for_active_run_to_end(service_id).await;
        let is_current = self.lock_and(|_, state| {
            let is_current = state.pending_starts.get(&service_id) == Some(&token);
            if is_current {
                state.pending_starts.remove(&service_id);
            }
            is_current
        });
        if !is_current {
            return self.status(service_id);
        }
        self.launch(service_id, requested_port, origin).await
    }

    /// Waits until a Dependency is Running or Exited, starting it if needed.
    fn await_dependency(
        self: Arc<Self>,
        service_id: Id,
        project: Project,
        origin: Origin,
    ) -> BoxFuture<DependencyOutcome> {
        Box::pin(async move {
            // A Dependency removed from the Project no longer holds anything up.
            let Some(key) = project.service(service_id).map(|service| service.key.clone()) else {
                return DependencyOutcome::Satisfied;
            };
            // Once the Dependency is (being) started for us, its ending Failed
            // or Stopped is final; before that, it is a state to start it from.
            let mut is_engaged = false;
            loop {
                enum Next {
                    Done(DependencyOutcome),
                    Wait(oneshot::Receiver<()>),
                    Start(Id),
                }
                let next = self.lock_and(|inner, state| match status_in(state, service_id).state {
                    ServiceState::Running | ServiceState::Exited => Next::Done(DependencyOutcome::Satisfied),
                    ServiceState::Starting => {
                        is_engaged = true;
                        Next::Wait(next_update(state, service_id))
                    }
                    ServiceState::Stopping => Next::Wait(next_update(state, service_id)),
                    ServiceState::Failed if is_engaged => Next::Done(DependencyOutcome::Failed(key.clone())),
                    ServiceState::Stopped if is_engaged => Next::Done(DependencyOutcome::Stopped),
                    ServiceState::Stopped | ServiceState::Failed => {
                        is_engaged = true;
                        match inner.begin_pending_start(state, service_id) {
                            Some(token) => Next::Start(token),
                            None => Next::Wait(next_update(state, service_id)),
                        }
                    }
                });
                match next {
                    Next::Done(outcome) => return outcome,
                    Next::Wait(update) => {
                        let _ = update.await;
                    }
                    Next::Start(token) => {
                        let operation =
                            self.clone().complete_pending_start(service_id, token, project.clone(), None, origin);
                        tokio::spawn(operation);
                    }
                }
            }
        })
    }

    /// Ends a start that is still waiting for its Dependencies as Stopped.
    /// Returns whether there was one.
    fn cancel_pending_start(&self, state: &mut State, service_id: Id) -> bool {
        if state.pending_starts.remove(&service_id).is_none() {
            return false;
        }
        let mut status = status_in(state, service_id);
        status.state = ServiceState::Stopped;
        status.awaited_dependencies.clear();
        update(state, status);
        true
    }

    // MARK: Operations

    async fn perform_start(
        self: Arc<Self>,
        service_id: Id,
        requested_port: Option<u16>,
        origin: Origin,
    ) -> ServiceStatus {
        let status = self.status(service_id);
        if matches!(status.state, ServiceState::Starting | ServiceState::Running) {
            return status;
        }
        self.launch(service_id, requested_port, origin).await
    }

    /// Launches a new Run once the previous one has fully ended.
    async fn launch(self: Arc<Self>, service_id: Id, requested_port: Option<u16>, origin: Origin) -> ServiceStatus {
        // A Run that ended on its own may still be stopping its leftovers.
        self.wait_for_active_run_to_end(service_id).await;

        let mut run = Run::new(service_id, origin);
        let launched: Result<ProcessTree, LocalhostError> = async {
            let (project, service) = self.library.service(service_id)?;
            let variables = self.library.resolved_variables(service_id)?;
            let fingerprint = launch_fingerprint(&service, &project, &variables);
            self.lock().launch_fingerprints.insert(service_id, fingerprint);
            if let Some(port) = service.port {
                let (port, change) = self.port_for_run(&service, port, &project, requested_port).await?;
                (run.port, run.port_change) = (Some(port), change);
            }
            let environment =
                RunEnvironment::merged(&RunEnvironment::base(&self.options.base_environment), &variables, run.port);
            let configuration = LaunchConfiguration::new(
                service.command_line.clone(),
                project.working_directory(&service),
                environment,
            );
            ProcessTree::launch(&configuration, self.options.stop_policy)
        }
        .await;
        let tree = match launched {
            Ok(tree) => tree,
            Err(failure) => {
                self.lock_and(|_, state| record_failed_launch(state, service_id, origin, failure));
                return self.status(service_id);
            }
        };

        self.lock_and(|_, state| {
            state.trees.insert(service_id, tree.clone());
            match run.port {
                Some(port) => state.run_ports.insert(service_id, port),
                None => state.run_ports.remove(&service_id),
            };
        });
        if let Some(start_time) = ProcessStartTime::of(tree.pid()) {
            self.run_registry.record(run.id, service_id, run.started_at, tree.pid(), start_time);
        }
        self.logs.begin_run(run.id, service_id);
        if let (Some(port), Some(change)) = (run.port, &run.port_change) {
            self.logs.append(&[OutputLine::new(OutputChannel::Localhost, log_note_for_port(port, change))], run.id);
        }
        self.lock_and(|_, state| {
            broadcast(state, ServiceEvent::RunStarted(run.clone()));
            update(
                state,
                ServiceStatus {
                    service_id,
                    state: ServiceState::Starting,
                    run: Some(run.clone()),
                    awaited_dependencies: Vec::new(),
                    is_restart_needed: false,
                },
            );
        });

        let run_id = run.id;
        tokio::spawn(self.clone().follow_output(service_id, run_id, tree.clone()));
        match run.port {
            Some(port) => {
                tokio::spawn(self.clone().watch_port(port, service_id, run_id));
            }
            None => {
                let inner = self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(inner.options.grace_period).await;
                    inner.lock_and(|_, state| {
                        let mut status = status_in(state, service_id);
                        if is_run(&status, run_id) && status.state == ServiceState::Starting {
                            status.state = ServiceState::Running;
                            update(state, status);
                        }
                    });
                });
            }
        }
        let inner = self.clone();
        tokio::spawn(async move {
            let termination = tree.wait_for_shell_exit().await;
            inner.shell_exited(service_id, run_id, termination);
            let exit = tree.wait_for_exit().await;
            inner.run_ended(service_id, run_id, exit);
        });
        self.status(service_id)
    }

    /// Logs the Run's output, watching it for a local URL and for errors that
    /// explain a failure (`diagnosis`), then how it ended.
    async fn follow_output(self: Arc<Self>, service_id: Id, run_id: Id, tree: ProcessTree) {
        let (mut has_detected_url, mut has_diagnosed_failure) = (false, false);
        while let Some(lines) = tree.next_output().await {
            self.logs.append(&lines, run_id);
            for line in lines.iter().filter(|line| line.channel != OutputChannel::Localhost) {
                if !has_detected_url
                    && line.text.contains("://")
                    && let Some(url) = LocalUrl::first(&Ansi::strip(&line.text))
                {
                    has_detected_url = true;
                    self.url_detected(url, service_id, run_id);
                }
                if !has_diagnosed_failure && let Some(failure) = diagnosis(line) {
                    has_diagnosed_failure = true;
                    self.failure_diagnosed(service_id, run_id, failure);
                }
            }
        }
        let exit = tree.wait_for_exit().await;
        let notes: Vec<OutputLine> =
            log_notes(&exit).into_iter().map(|note| OutputLine::new(OutputChannel::Localhost, note)).collect();
        self.logs.append(&notes, run_id);
        self.logs.end_run(run_id);
    }

    async fn perform_stop(self: Arc<Self>, service_id: Id, origin: Origin) -> ServiceStatus {
        let target = self.lock_and(|_, state| {
            let mut status = status_in(state, service_id);
            if !matches!(status.state, ServiceState::Starting | ServiceState::Running) {
                return None;
            }
            let tree = state.trees.get(&service_id)?.clone();
            let run_id = status.run.as_ref()?.id;
            status.state = ServiceState::Stopping;
            if let Some(run) = &mut status.run {
                run.stop_origin = Some(origin);
            }
            update(state, status);
            Some((tree, run_id))
        });
        let Some((tree, run_id)) = target else { return self.status(service_id) };
        let exit = tree.stop().await;
        self.run_ended(service_id, run_id, exit);
        self.status(service_id)
    }

    /// Like `perform_stop`, but also ends a Run whose shell already exited and
    /// whose leftovers are still being cleaned up.
    async fn perform_stop_for_quit(self: Arc<Self>, service_id: Id, origin: Origin) -> ServiceStatus {
        let state = self.status(service_id).state;
        if matches!(state, ServiceState::Starting | ServiceState::Running) {
            return self.perform_stop(service_id, origin).await;
        }
        let target = self.lock_and(|_, state| {
            let tree = state.trees.get(&service_id)?.clone();
            Some((tree, state.statuses.get(&service_id)?.run.as_ref()?.id))
        });
        let Some((tree, run_id)) = target else { return self.status(service_id) };
        let exit = tree.stop().await;
        self.run_ended(service_id, run_id, exit);
        self.status(service_id)
    }

    async fn wait_for_active_run_to_end(&self, service_id: Id) {
        let target = self.lock_and(|_, state| {
            let tree = state.trees.get(&service_id)?.clone();
            Some((tree, state.statuses.get(&service_id)?.run.as_ref()?.id))
        });
        let Some((tree, run_id)) = target else { return };
        let exit = tree.wait_for_exit().await;
        self.run_ended(service_id, run_id, exit);
    }

    // MARK: Restart to apply

    /// Marks each live Run of the Project whose launch configuration changed.
    async fn configuration_changed(&self, project: &Project) {
        for service in &project.services {
            let launched = self.lock_and(|_, state| {
                if !state.trees.contains_key(&service.id) {
                    return None;
                }
                let fingerprint = *state.launch_fingerprints.get(&service.id)?;
                Some((fingerprint, state.statuses.get(&service.id)?.run.as_ref()?.id))
            });
            let Some((launched, run_id)) = launched else { continue };
            // A Secret that can't be read any more is a change, too.
            let current = self
                .library
                .resolved_variables(service.id)
                .ok()
                .map(|variables| launch_fingerprint(service, project, &variables));
            self.lock_and(|_, state| {
                let mut status = status_in(state, service.id);
                if !is_run(&status, run_id) || !matches!(status.state, ServiceState::Starting | ServiceState::Running) {
                    return;
                }
                status.is_restart_needed = current != Some(launched);
                update(state, status);
            });
        }
    }

    // MARK: Transitions

    /// Keeps a Run Starting until its Port accepts connections. After the
    /// timeout it is Running with `is_port_unresponsive`, and the Port is still
    /// watched so the warning clears once it opens.
    async fn watch_port(self: Arc<Self>, port: u16, service_id: Id, run_id: Id) {
        let deadline = Instant::now() + self.options.port_timeout;
        loop {
            let status = self.status(service_id);
            if !is_run(&status, run_id) || !matches!(status.state, ServiceState::Starting | ServiceState::Running) {
                return;
            }
            if crate::port_probe::is_accepting(port).await {
                self.lock_and(|_, state| {
                    let mut status = status_in(state, service_id);
                    if is_run(&status, run_id) && matches!(status.state, ServiceState::Starting | ServiceState::Running)
                    {
                        status.state = ServiceState::Running;
                        if let Some(run) = &mut status.run {
                            run.is_port_unresponsive = false;
                        }
                        update(state, status);
                    }
                });
                return;
            }
            if Instant::now() >= deadline {
                self.lock_and(|_, state| {
                    let mut status = status_in(state, service_id);
                    if is_run(&status, run_id) && status.state == ServiceState::Starting {
                        status.state = ServiceState::Running;
                        if let Some(run) = &mut status.run {
                            run.is_port_unresponsive = true;
                        }
                        update(state, status);
                    }
                });
            }
            tokio::time::sleep(self.options.port_probe_interval).await;
        }
    }

    fn url_detected(&self, url: String, service_id: Id, run_id: Id) {
        self.lock_and(|_, state| {
            let mut status = status_in(state, service_id);
            match &mut status.run {
                Some(run) if run.id == run_id && run.ended_at.is_none() => run.detected_url = Some(url),
                _ => return,
            }
            update(state, status);
        });
    }

    /// The Run printed an error that explains a failure (e.g. Docker's "daemon
    /// not running"). If it fails (or already has), that is the reason given.
    fn failure_diagnosed(&self, service_id: Id, run_id: Id, failure: LocalhostError) {
        self.lock_and(|_, state| {
            let mut status = status_in(state, service_id);
            let Some(run) = &mut status.run else { return };
            if run.id != run_id {
                return;
            }
            if run.ended_at.is_none() {
                state.diagnosed_failures.insert(run_id, failure.clone());
            }
            if status.state != ServiceState::Failed || run.failure.is_some() {
                return;
            }
            run.failure = Some(failure);
            update(state, status);
        });
    }

    /// The shell exited: the Service is no longer up, even if leftovers in its
    /// process group are still being stopped.
    fn shell_exited(&self, service_id: Id, run_id: Id, termination: Termination) {
        self.lock_and(|_, state| {
            let mut status = status_in(state, service_id);
            if !status.run.as_ref().is_some_and(|run| run.id == run_id && run.ended_at.is_none()) {
                return;
            }
            status.state = state_after_ending(status.state, termination);
            let failure = if status.state == ServiceState::Failed {
                failure_of(state, run_id, Some(termination))
            } else {
                termination.launch_error()
            };
            if let Some(run) = &mut status.run {
                run.termination = Some(termination);
                run.failure = failure;
            }
            update(state, status);
        });
    }

    /// The whole process tree has ended. Safe to call more than once per Run.
    fn run_ended(&self, service_id: Id, run_id: Id, exit: ProcessTreeExit) {
        self.lock_and(|inner, state| {
            let mut status = status_in(state, service_id);
            let mut run = status.run.clone().filter(|run| run.id == run_id && run.ended_at.is_none())?;
            run.ended_at = Some(SystemTime::now());
            run.termination = Some(exit.termination);
            run.was_force_killed = exit.was_force_killed();
            // A pending start stays Starting while the previous Run's leftovers end.
            if !state.pending_starts.contains_key(&service_id) {
                status.state = state_after_ending(status.state, exit.termination);
            }
            run.failure = if status.state == ServiceState::Failed {
                failure_of(state, run_id, Some(exit.termination))
            } else {
                exit.termination.launch_error()
            };
            state.diagnosed_failures.remove(&run_id);
            status.run = Some(run.clone());
            state.trees.remove(&service_id);
            state.run_ports.remove(&service_id);
            // Before anyone hears of the end, so no one finds the record of an ended Run.
            inner.run_registry.remove(run_id);
            update(state, status);
            broadcast(state, ServiceEvent::RunEnded(run));
            Some(())
        });
    }
}

// MARK: State helpers (called with the state locked)

fn status_in(state: &State, service_id: Id) -> ServiceStatus {
    state.statuses.get(&service_id).cloned().unwrap_or_else(|| ServiceStatus::never_run(service_id))
}

fn is_run(status: &ServiceStatus, run_id: Id) -> bool {
    status.run.as_ref().is_some_and(|run| run.id == run_id)
}

/// A receiver that completes at the Service's next status update.
fn next_update(state: &mut State, service_id: Id) -> oneshot::Receiver<()> {
    let (sender, receiver) = oneshot::channel();
    state.update_waiters.entry(service_id).or_default().push(sender);
    receiver
}

/// Stores `status`, broadcasting `ServiceStateChanged` if its state, Run,
/// awaited Dependencies, Port warning, detected URL, failure or restart need changed.
fn update(state: &mut State, mut status: ServiceStatus) {
    if !matches!(status.state, ServiceState::Starting | ServiceState::Running) {
        status.is_restart_needed = false;
    }
    let service_id = status.service_id;
    let previous = state.statuses.get(&service_id);
    let run_field = |status: Option<&ServiceStatus>| {
        status
            .and_then(|status| status.run.as_ref())
            .map(|run| (run.id, run.is_port_unresponsive, run.detected_url.clone(), run.failure.clone()))
    };
    let changed = previous.is_none_or(|previous| {
        previous.state != status.state
            || previous.awaited_dependencies != status.awaited_dependencies
            || previous.is_restart_needed != status.is_restart_needed
    }) || run_field(previous) != run_field(Some(&status));
    if changed {
        broadcast(state, ServiceEvent::ServiceStateChanged(status.clone()));
    }
    state.statuses.insert(service_id, status);
    for waiter in state.update_waiters.remove(&service_id).unwrap_or_default() {
        let _ = waiter.send(());
    }
}

fn broadcast(state: &mut State, event: ServiceEvent) {
    state.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
}

/// Records a Run that could not be launched at all, leaving the Service Failed.
fn record_failed_launch(state: &mut State, service_id: Id, origin: Origin, failure: LocalhostError) {
    let mut run = Run::new(service_id, origin);
    run.failure = Some(failure);
    run.ended_at = Some(SystemTime::now());
    broadcast(state, ServiceEvent::RunStarted(run.clone()));
    update(
        state,
        ServiceStatus {
            service_id,
            state: ServiceState::Failed,
            run: Some(run.clone()),
            awaited_dependencies: Vec::new(),
            is_restart_needed: false,
        },
    );
    broadcast(state, ServiceEvent::RunEnded(run));
}

/// Why a Run that ended with `termination` failed, if there is a typed reason.
fn failure_of(state: &State, run_id: Id, termination: Option<Termination>) -> Option<LocalhostError> {
    termination
        .and_then(|termination| termination.launch_error())
        .or_else(|| state.diagnosed_failures.get(&run_id).cloned())
}

/// The state a Service moves to when its command ends while in `state`.
fn state_after_ending(state: ServiceState, termination: Termination) -> ServiceState {
    match state {
        ServiceState::Stopping => ServiceState::Stopped,
        // A one-shot command (e.g. a migration) may finish within Starting.
        ServiceState::Starting | ServiceState::Running => {
            if termination == (Termination::Exited { code: 0 }) {
                ServiceState::Exited
            } else {
                ServiceState::Failed
            }
        }
        other => other,
    }
}

fn launch_fingerprint(service: &Service, project: &Project, variables: &BTreeMap<String, String>) -> u64 {
    let mut hasher = DefaultHasher::new();
    service.command_line.hash(&mut hasher);
    project.working_directory(service).hash(&mut hasher);
    variables.hash(&mut hasher);
    service.port.hash(&mut hasher);
    hasher.finish()
}

/// What localhost writes into a Run's log when it starts on another port.
fn log_note_for_port(port: u16, change: &PortChange) -> String {
    match &change.reason {
        PortChangeReason::PortInUse { holder: Some(holder) } => format!(
            "Port {} is in use by {} (PID {}): starting on port {port} instead",
            change.configured_port, holder.name, holder.pid
        ),
        PortChangeReason::PortInUse { holder: None } => {
            format!("Port {} is in use: starting on port {port} instead", change.configured_port)
        }
        PortChangeReason::Requested => {
            format!("Starting on port {port} instead of {}, as requested", change.configured_port)
        }
    }
}

/// What localhost writes into a Run's log once it has ended.
fn log_notes(exit: &ProcessTreeExit) -> Vec<String> {
    let mut notes = Vec::new();
    if exit.was_force_killed() {
        notes.push("Force-killed: the process group did not stop in time".to_string());
    }
    notes.push(exit.termination.summary());
    notes
}
