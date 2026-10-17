//! Where each Project's Services run, behind one interface. This
//! machine is a Service manager in this process; any other Execution Target
//! (a WSL distro) is a `localhostd` driven through a [`Remote`], connected
//! the first time something needs it. A client reaches them all the same way.
//!
//! The client keeps the one `ProjectLibrary`. Each connected `localhostd`
//! is sent the Projects that run there, with their Secret values, whenever
//! they change and before every start. The statuses and events of every
//! target are merged, so a client observes them as it would one
//! `ServiceManager`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::SystemTime;

use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::remote::Remote;
use crate::{
    ExecutionTarget, Id, Listener, LocalhostError, LogStore, Origin, Orphan, OutputLine, Project, ProjectLibrary,
    ProjectScanner, RunRegistry, RunStats, ServiceEvent, ServiceManager, ServiceManagerOptions, ServiceOutputLine,
    ServiceState, ServiceStatus, ServiceSuggestion,
};

/// A connection being made to a `localhostd`, e.g. `Remote::spawn(…)`.
pub type Connecting = Pin<Box<dyn Future<Output = Result<Remote, LocalhostError>> + Send>>;

/// How this client reaches an Execution Target.
pub enum Link {
    /// Through the Service manager in this process.
    InProcess,
    /// Through a `localhostd`, once this connects.
    Remote(Connecting),
    /// Not from here: calls fail with `TargetUnavailable(reason)`.
    Unavailable(String),
}

/// Says how to reach a target. Asked again whenever a target isn't connected,
/// so a `localhostd` that was lost is started anew.
pub type Launcher = Box<dyn Fn(&ExecutionTarget) -> Link + Send + Sync>;

/// Orphans that a `localhostd` found as it connected: Runs of an earlier
/// session there that ended unexpectedly.
#[derive(Clone, Debug)]
pub struct FoundOrphans {
    pub target: ExecutionTarget,
    pub orphans: Vec<Orphan>,
}

/// Every Execution Target, as one Service manager. Cheap to clone. Must be
/// used inside a tokio runtime.
#[derive(Clone)]
pub struct Targets {
    inner: Arc<Inner>,
}

struct Inner {
    library: Arc<ProjectLibrary>,
    launcher: Launcher,
    local: Local,
    slots: Mutex<HashMap<ExecutionTarget, Arc<Slot>>>,
    hub: Mutex<Hub>,
    automatic_port_move: AtomicBool,
    next_source: AtomicU64,
    /// Where tasks go when a call comes from outside the runtime.
    runtime: tokio::runtime::Handle,
}

struct Local {
    manager: ServiceManager,
    logs: Arc<LogStore>,
    runs: Arc<RunRegistry>,
}

/// One target's connection.
#[derive(Default)]
struct Slot {
    remote: Mutex<Option<Remote>>,
    /// Held while connecting, so a target is connected once at a time.
    connecting: tokio::sync::Mutex<()>,
    /// Held while sending Projects, so they arrive in the order they were read.
    sending: tokio::sync::Mutex<()>,
}

impl Slot {
    fn connected(&self) -> Option<Remote> {
        let remote = self.remote.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        remote.as_ref().filter(|remote| remote.is_connected()).cloned()
    }
}

/// The merged statuses, and who hears about them.
#[derive(Default)]
struct Hub {
    statuses: HashMap<Id, ServiceStatus>,
    /// The connection each status came from, so losing one fails only its Runs.
    sources: HashMap<Id, u64>,
    subscribers: Vec<mpsc::UnboundedSender<ServiceEvent>>,
    orphan_subscribers: Vec<mpsc::UnboundedSender<FoundOrphans>>,
}

/// What a call reaches.
enum Reached<'a> {
    Local(&'a Local),
    Remote(Remote),
}

const LOST: &str = "the connection to localhostd closed";

impl Targets {
    /// This machine's Runs keep their logs and records in `root`. Asks
    /// `launcher` how to reach every other target.
    pub fn open(
        library: Arc<ProjectLibrary>,
        root: &Path,
        options: ServiceManagerOptions,
        launcher: Launcher,
    ) -> Targets {
        let logs = Arc::new(LogStore::new(root));
        let runs = Arc::new(RunRegistry::new(root));
        let automatic_port_move = AtomicBool::new(options.automatic_port_move);
        let manager = ServiceManager::new(library.clone(), logs.clone(), runs.clone(), options);
        let local_statuses = manager.subscribe();
        let mut library_events = library.events();
        let inner = Arc::new(Inner {
            library,
            launcher,
            local: Local { manager, logs, runs },
            slots: Mutex::new(HashMap::new()),
            hub: Mutex::new(Hub::default()),
            automatic_port_move,
            next_source: AtomicU64::new(0),
            runtime: tokio::runtime::Handle::current(),
        });
        inner.attach(local_statuses, false);

        // Sends changed Projects to every connected target, so a Run there
        // shows "Restart to apply" as one here does.
        let weak = Arc::downgrade(&inner);
        tokio::spawn(async move {
            while library_events.recv().await.is_some() {
                while library_events.try_recv().is_ok() {}
                let Some(inner) = weak.upgrade() else { return };
                for (target, remote) in inner.connected() {
                    let _ = inner.send_projects(&target, &remote).await;
                }
            }
        });
        Targets { inner }
    }

    pub fn library(&self) -> &Arc<ProjectLibrary> {
        &self.inner.library
    }

    // MARK: Observing

    pub fn status(&self, service_id: Id) -> ServiceStatus {
        let hub = self.inner.hub();
        hub.statuses.get(&service_id).cloned().unwrap_or_else(|| ServiceStatus::never_run(service_id))
    }

    /// The status of every Service that has run on any target, plus a stream
    /// of every change after that snapshot.
    pub fn subscribe(&self) -> (HashMap<Id, ServiceStatus>, mpsc::UnboundedReceiver<ServiceEvent>) {
        let mut hub = self.inner.hub();
        let (sender, receiver) = mpsc::unbounded_channel();
        hub.subscribers.push(sender);
        (hub.statuses.clone(), receiver)
    }

    /// The Orphans each `localhostd` finds as it connects.
    pub fn found_orphans(&self) -> mpsc::UnboundedReceiver<FoundOrphans> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.inner.hub().orphan_subscribers.push(sender);
        receiver
    }

    // MARK: Lifecycle, as `ServiceManager`

    pub async fn start(
        &self,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        match self.inner.reach_service(service_id, true).await? {
            Reached::Local(local) => local.manager.start(service_id, on_port, origin).await,
            Reached::Remote(remote) => remote.start(service_id, on_port, origin).await,
        }
    }

    pub async fn stop(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        match self.inner.reach_service(service_id, false).await? {
            Reached::Local(local) => local.manager.stop(service_id, origin).await,
            Reached::Remote(remote) => remote.stop(service_id, origin).await,
        }
    }

    pub async fn restart(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        match self.inner.reach_service(service_id, true).await? {
            Reached::Local(local) => local.manager.restart(service_id, origin).await,
            Reached::Remote(remote) => remote.restart(service_id, origin).await,
        }
    }

    pub async fn start_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let target = self.inner.library.project(project_id)?.execution_target;
        match self.inner.reach(&target, true).await? {
            Reached::Local(local) => local.manager.start_all(project_id, origin).await,
            Reached::Remote(remote) => remote.start_all(project_id, origin).await,
        }
    }

    pub async fn stop_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        let target = self.inner.library.project(project_id)?.execution_target;
        match self.inner.reach(&target, false).await? {
            Reached::Local(local) => local.manager.stop_all(project_id, origin).await,
            Reached::Remote(remote) => remote.stop_all(project_id, origin).await,
        }
    }

    /// Stops every Run on this machine and on every connected target, e.g.
    /// before quitting. Returns once all of them have ended, or their
    /// connection was lost.
    pub async fn stop_every_run(&self, origin: Origin) {
        let mut stops = JoinSet::new();
        for (_, remote) in self.inner.connected() {
            stops.spawn(async move {
                let _ = remote.stop_every_run(origin).await;
            });
        }
        self.inner.local.manager.stop_every_run(origin).await;
        stops.join_all().await;
    }

    /// An app setting, on by default; a target connected later gets it too.
    /// May be called outside the runtime.
    pub fn set_automatic_port_move(&self, is_enabled: bool) {
        self.inner.automatic_port_move.store(is_enabled, Ordering::Relaxed);
        self.inner.local.manager.set_automatic_port_move(is_enabled);
        for (_, remote) in self.inner.connected() {
            self.inner.runtime.spawn(async move {
                let _ = remote.set_automatic_port_move(is_enabled).await;
            });
        }
    }

    // MARK: Listeners

    /// The Listeners of this machine and of every connected target, each
    /// with where it is. A target that can't be reached adds none.
    pub async fn listeners(&self) -> Vec<(ExecutionTarget, Listener)> {
        let mut found = Vec::new();
        let mut targets = vec![ExecutionTarget::ThisMachine];
        let connected = self.inner.connected().into_iter().map(|(target, _)| target);
        targets.extend(connected.filter(|target| !target.is_this_machine()));
        for target in targets {
            let listeners = match self.inner.reach(&target, false).await {
                Ok(Reached::Local(local)) => local.manager.listeners().await,
                Ok(Reached::Remote(remote)) => remote.listeners().await.unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            found.extend(listeners.into_iter().map(|listener| (target.clone(), listener)));
        }
        found
    }

    /// Stops a Listener on `target`, as `ServiceManager::stop_listener`.
    pub async fn stop_listener(
        &self,
        target: &ExecutionTarget,
        listener: &Listener,
        origin: Origin,
    ) -> Result<(), LocalhostError> {
        match self.inner.reach(target, false).await? {
            Reached::Local(local) => local.manager.stop_listener(listener, origin).await,
            Reached::Remote(remote) => remote.stop_listener(listener, origin).await,
        }
    }

    pub async fn next_free_port(&self, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        match self.inner.reach_service(service_id, false).await? {
            Reached::Local(local) => local.manager.next_free_port(service_id).await,
            Reached::Remote(remote) => remote.next_free_port(service_id).await,
        }
    }

    // MARK: Stats

    /// What the Service's active Run uses, as `ServiceManager::run_stats`,
    /// measured where it runs: a WSL distro's processes only its own
    /// `localhostd` can see.
    pub async fn run_stats(
        &self,
        service_id: Id,
        previous: Option<RunStats>,
    ) -> Result<Option<RunStats>, LocalhostError> {
        match self.inner.reach_service(service_id, false).await? {
            Reached::Local(local) => local.manager.run_stats(service_id, previous).await,
            Reached::Remote(remote) => remote.run_stats(service_id, previous).await,
        }
    }

    // MARK: Orphans

    /// This machine's Orphans, as `RunRegistry::orphans`: call it at launch,
    /// before anything starts. Other targets report theirs through
    /// `found_orphans` as they connect.
    pub fn orphans(&self) -> Vec<Orphan> {
        self.inner.local.runs.orphans()
    }

    pub async fn stop_orphans(&self, target: &ExecutionTarget, orphans: &[Orphan]) -> Result<(), LocalhostError> {
        match self.inner.reach(target, false).await? {
            Reached::Local(local) => {
                local.runs.stop_orphans(orphans, Default::default()).await;
                Ok(())
            }
            Reached::Remote(remote) => remote.stop_orphans(orphans).await,
        }
    }

    pub async fn leave_orphans(&self, target: &ExecutionTarget, orphans: &[Orphan]) -> Result<(), LocalhostError> {
        match self.inner.reach(target, false).await? {
            Reached::Local(local) => {
                local.runs.leave_orphans(orphans);
                Ok(())
            }
            Reached::Remote(remote) => remote.leave_orphans(orphans).await,
        }
    }

    // MARK: Logs, as `LogStore`

    /// Removes the Service's logs where it ran. `target` is given since the
    /// Service may already be removed from the library.
    pub async fn remove_logs(&self, target: &ExecutionTarget, service_id: Id) -> Result<(), LocalhostError> {
        match self.inner.reach(target, false).await? {
            Reached::Local(local) => {
                local.logs.remove_logs(service_id);
                Ok(())
            }
            Reached::Remote(remote) => remote.remove_logs(service_id).await,
        }
    }

    pub async fn clear_buffer(&self, service_id: Id) -> Result<(), LocalhostError> {
        match self.inner.reach_service(service_id, false).await? {
            Reached::Local(local) => {
                local.logs.clear_buffer(service_id);
                Ok(())
            }
            Reached::Remote(remote) => remote.clear_buffer(service_id).await,
        }
    }

    /// A Run's buffered lines plus a stream of every line after them, from
    /// the target its Service runs on.
    pub async fn subscribe_to_run(
        &self,
        run_id: Id,
    ) -> Result<(Vec<OutputLine>, mpsc::UnboundedReceiver<OutputLine>), LocalhostError> {
        let service_id = {
            let hub = self.inner.hub();
            hub.statuses
                .values()
                .find(|status| status.run.as_ref().is_some_and(|run| run.id == run_id))
                .map(|s| s.service_id)
        };
        let target = service_id.map_or(ExecutionTarget::ThisMachine, |id| self.inner.target_of_service(id));
        match self.inner.reach(&target, false).await? {
            Reached::Local(local) => Ok(local.logs.subscribe_to_run(run_id)),
            Reached::Remote(remote) => remote.subscribe_to_run(run_id).await,
        }
    }

    /// The combined stream of several Services, as
    /// `LogStore::subscribe_to_services`, across the targets they run on.
    pub async fn subscribe_to_services(
        &self,
        service_ids: HashSet<Id>,
    ) -> Result<(Vec<ServiceOutputLine>, mpsc::UnboundedReceiver<ServiceOutputLine>), LocalhostError> {
        let mut by_target: HashMap<ExecutionTarget, HashSet<Id>> = HashMap::new();
        for id in service_ids {
            by_target.entry(self.inner.target_of_service(id)).or_default().insert(id);
        }
        let mut parts = Vec::new();
        for (target, ids) in by_target {
            parts.push(match self.inner.reach(&target, false).await? {
                Reached::Local(local) => local.logs.subscribe_to_services(ids),
                Reached::Remote(remote) => remote.subscribe_to_services(ids).await?,
            });
        }
        if parts.len() <= 1 {
            return Ok(parts.pop().unwrap_or_else(|| (Vec::new(), mpsc::unbounded_channel().1)));
        }
        let (sender, merged) = mpsc::unbounded_channel();
        let mut buffered = Vec::new();
        for (lines, mut receiver) in parts {
            buffered.extend(lines);
            let sender = sender.clone();
            tokio::spawn(async move {
                while let Some(line) = receiver.recv().await {
                    if sender.send(line).is_err() {
                        return;
                    }
                }
            });
        }
        buffered.sort_by_key(|line| line.line.timestamp);
        Ok((buffered, merged))
    }

    // MARK: Detection

    /// Suggestions for a folder on `target`, as `ProjectScanner::scan_folder`.
    pub async fn scan_folder(
        &self,
        target: &ExecutionTarget,
        path: &str,
    ) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        match self.inner.reach(target, false).await? {
            Reached::Local(_) => {
                let path = path.to_string();
                Ok(blocking(move || ProjectScanner::default().scan_folder(&path)).await)
            }
            Reached::Remote(remote) => remote.scan_folder(path).await,
        }
    }

    /// As `ProjectScanner::scan_project`, where the Project runs.
    pub async fn scan_project(&self, project: &Project) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        match self.inner.reach(&project.execution_target, false).await? {
            Reached::Local(_) => {
                let project = project.clone();
                Ok(blocking(move || ProjectScanner::default().scan_project(&project)).await)
            }
            Reached::Remote(remote) => remote.scan_project(project).await,
        }
    }
}

async fn blocking<T: Default + Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work).await.unwrap_or_default()
}

impl Inner {
    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn slot(&self, target: &ExecutionTarget) -> Arc<Slot> {
        let mut slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        slots.entry(target.clone()).or_default().clone()
    }

    /// Where a Service runs; this machine for one the library doesn't have.
    fn target_of_service(&self, service_id: Id) -> ExecutionTarget {
        self.library.service(service_id).map(|(project, _)| project.execution_target).unwrap_or_default()
    }

    async fn reach_service(self: &Arc<Self>, service_id: Id, is_start: bool) -> Result<Reached<'_>, LocalhostError> {
        let (project, _) = self.library.service(service_id)?;
        self.reach(&project.execution_target, is_start).await
    }

    /// The way to `target`, connecting to it if needed. Before a start, a
    /// `localhostd` is sent the current Projects first, so it starts what
    /// the user just saved.
    async fn reach(self: &Arc<Self>, target: &ExecutionTarget, is_start: bool) -> Result<Reached<'_>, LocalhostError> {
        let slot = self.slot(target);
        let one_at_a_time = slot.connecting.lock().await;
        if let Some(remote) = slot.connected() {
            drop(one_at_a_time);
            if is_start {
                self.send_projects(target, &remote).await?;
            }
            return Ok(Reached::Remote(remote));
        }
        let connecting = match (self.launcher)(target) {
            Link::InProcess => return Ok(Reached::Local(&self.local)),
            Link::Unavailable(reason) => return Err(LocalhostError::TargetUnavailable(reason)),
            Link::Remote(connecting) => connecting,
        };
        let remote = connecting.await?;
        remote.set_automatic_port_move(self.automatic_port_move.load(Ordering::Relaxed)).await?;
        self.send_projects(target, &remote).await?;
        let orphans = remote.orphans().await?;
        self.attach(remote.subscribe(), true);
        *slot.remote.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(remote.clone());
        drop(one_at_a_time);
        if !orphans.is_empty() {
            let found = FoundOrphans { target: target.clone(), orphans };
            self.hub().orphan_subscribers.retain(|subscriber| subscriber.send(found.clone()).is_ok());
        }
        Ok(Reached::Remote(remote))
    }

    fn connected(&self) -> Vec<(ExecutionTarget, Remote)> {
        let slots = self.slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        slots.iter().filter_map(|(target, slot)| Some((target.clone(), slot.connected()?))).collect()
    }

    /// Sends `remote` the Projects that run on `target`, with the values of
    /// their Secrets.
    async fn send_projects(&self, target: &ExecutionTarget, remote: &Remote) -> Result<(), LocalhostError> {
        let slot = self.slot(target);
        let _sending = slot.sending.lock().await;
        let projects: Vec<Project> =
            self.library.projects().into_iter().filter(|project| project.execution_target == *target).collect();
        let mut secrets = HashMap::new();
        for project in &projects {
            secrets.extend(self.library.stored_secrets(project.id)?);
        }
        remote.set_projects(projects, secrets).await
    }

    /// Merges a target's statuses and events into the hub. When a remote's
    /// events end, its connection is lost: its active Runs are reported
    /// Failed, as nothing can be known of them any more.
    fn attach(
        self: &Arc<Self>,
        (statuses, mut events): (HashMap<Id, ServiceStatus>, mpsc::UnboundedReceiver<ServiceEvent>),
        is_remote: bool,
    ) {
        let source = self.next_source.fetch_add(1, Ordering::Relaxed);
        {
            let mut hub = self.hub();
            for status in statuses.into_values() {
                hub.publish(ServiceEvent::ServiceStateChanged(status), source);
            }
        }
        let weak: Weak<Inner> = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let Some(inner) = weak.upgrade() else { return };
                inner.hub().publish(event, source);
            }
            let Some(inner) = weak.upgrade().filter(|_| is_remote) else { return };
            let mut hub = inner.hub();
            let lost: Vec<ServiceStatus> = hub
                .statuses
                .values()
                .filter(|status| hub.sources.get(&status.service_id) == Some(&source))
                .filter_map(lost_status)
                .collect();
            for status in lost {
                hub.publish(ServiceEvent::ServiceStateChanged(status), source);
            }
        });
    }
}

impl Hub {
    fn publish(&mut self, event: ServiceEvent, source: u64) {
        if let ServiceEvent::ServiceStateChanged(status) = &event {
            self.statuses.insert(status.service_id, status.clone());
            self.sources.insert(status.service_id, source);
        }
        self.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
}

/// An active Service's status once its target's connection is lost: Failed
/// with `TargetUnavailable` if its Run was alive, else Stopped.
fn lost_status(status: &ServiceStatus) -> Option<ServiceStatus> {
    if !status.state.is_active() {
        return None;
    }
    let mut status = status.clone();
    status.awaited_dependencies.clear();
    status.is_restart_needed = false;
    match status.run.as_mut().filter(|run| run.ended_at.is_none()) {
        Some(run) => {
            run.ended_at = Some(SystemTime::now());
            run.failure = Some(LocalhostError::TargetUnavailable(LOST.into()));
            status.state = ServiceState::Failed;
        }
        None => status.state = ServiceState::Stopped,
    }
    Some(status)
}
