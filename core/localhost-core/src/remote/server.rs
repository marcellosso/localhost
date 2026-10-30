//! The core's side: a Service manager, logs and Run records on this
//! machine, driven by the requests of each connection to it: the one app
//! on `localhostd --stdio`, or every client of the shared core.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::Poll;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{Call, Client, Hello, Message, Request};
use crate::library::all_secret_accounts;
use crate::{
    Id, LocalhostError, LogStore, MemorySecretStore, ProjectLibrary, ProjectScanner, RunRegistry, ServiceEvent,
    ServiceManager, ServiceManagerOptions, StopPolicy,
};

/// How often log lines cross, at most: once per frame.
const LOG_BATCH_INTERVAL: Duration = Duration::from_millis(16);

/// How long a closed connection's last messages may take to go out. A
/// named pipe whose client has gone may never fail the write that would
/// end them, and the shared core counts a connection closed only after.
const LAST_MESSAGES_TIMEOUT: Duration = Duration::from_secs(1);

/// What every connection shares.
pub(crate) struct Core {
    secrets: Arc<MemorySecretStore>,
    library: Arc<ProjectLibrary>,
    logs: Arc<LogStore>,
    runs: Arc<RunRegistry>,
    manager: ServiceManager,
    stop_policy: StopPolicy,
    connections: Mutex<Connections>,
    /// A client asked it to exit once none is connected (`Request::Retire`).
    is_retiring: AtomicBool,
}

#[derive(Default)]
struct Connections {
    next_id: u64,
    clients: HashMap<u64, Client>,
    /// How to reach each connection to dismiss it: its output, and what ends its reading.
    dismissers: HashMap<u64, (mpsc::UnboundedSender<Message>, Arc<tokio::sync::Notify>)>,
    /// The connections that need each Service's Run: the one that started
    /// it, and those that started a Service depending on it.
    holders: HashMap<Id, HashSet<u64>>,
}

/// One connection: its output, and what it subscribed to.
struct Connection {
    core: Arc<Core>,
    id: u64,
    output: mpsc::UnboundedSender<Message>,
    /// The Service manager's events not sent yet. Every reply sends them
    /// first, so the client has a call's events by the time it has its reply.
    events: Mutex<Option<mpsc::UnboundedReceiver<ServiceEvent>>>,
    log_streams: Mutex<HashMap<u64, JoinHandle<()>>>,
}

/// Answers the requests on `input` until it closes, then stops every Run.
/// Runs' logs and records are kept under `storage_root`; Projects and
/// Secrets only in memory. Needs a multi-threaded tokio runtime.
pub async fn serve<R, W>(input: R, output: W, storage_root: &Path, options: ServiceManagerOptions)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    Core::new(storage_root, options).serve_connection(input, output).await;
}

impl Core {
    pub(crate) fn new(storage_root: &Path, options: ServiceManagerOptions) -> Arc<Core> {
        let secrets = Arc::new(MemorySecretStore::new());
        let library = Arc::new(ProjectLibrary::mirror(secrets.clone()));
        let logs = Arc::new(LogStore::new(storage_root));
        let runs = Arc::new(RunRegistry::new(storage_root));
        let stop_policy = options.stop_policy;
        let manager = ServiceManager::new(library.clone(), logs.clone(), runs.clone(), options);
        Arc::new(Core {
            secrets,
            library,
            logs,
            runs,
            manager,
            stop_policy,
            connections: Mutex::new(Connections::default()),
            is_retiring: AtomicBool::new(false),
        })
    }

    fn connections(&self) -> MutexGuard<'_, Connections> {
        self.connections.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The connections of coding agents.
    fn agents(&self) -> Vec<u64> {
        let connections = self.connections();
        connections.clients.iter().filter(|(_, client)| **client == Client::Agent).map(|(id, _)| *id).collect()
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.connections().clients.len()
    }

    pub(crate) fn is_retiring(&self) -> bool {
        self.is_retiring.load(Ordering::Relaxed)
    }

    pub(crate) async fn stop_every_run(&self) {
        self.connections().holders.clear();
        self.manager.stop_every_run(Client::App.origin()).await;
    }

    /// Answers the requests on `input` until it closes, then stops the Runs
    /// nobody else needs.
    pub(crate) async fn serve_connection<R, W>(self: &Arc<Self>, input: R, output: W)
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut writer = tokio::spawn(write_messages(receiver, output));
        let _ = sender.send(Message::Hello(Hello::current()));
        let dismissed = Arc::new(tokio::sync::Notify::new());
        let id = {
            let mut connections = self.connections();
            connections.next_id += 1;
            let id = connections.next_id;
            connections.clients.insert(id, Client::default());
            connections.dismissers.insert(id, (sender.clone(), dismissed.clone()));
            id
        };
        let connection = Arc::new(Connection {
            core: self.clone(),
            id,
            output: sender,
            events: Mutex::new(None),
            log_streams: Mutex::new(HashMap::new()),
        });

        let mut lines = BufReader::new(input).lines();
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line,
                _ = dismissed.notified() => break,
            };
            let Ok(Some(line)) = line else { break };
            match serde_json::from_str::<Call>(&line) {
                Ok(Call { id, request }) => {
                    let connection = connection.clone();
                    tokio::spawn(async move {
                        if let Request::Subscribe = request {
                            connection.subscribe(id);
                        } else {
                            let result = connection.handle(request).await;
                            connection.reply(id, result);
                        }
                    });
                }
                Err(error) => eprintln!("localhostd: ignoring a request it can't read: {error}"),
            }
        }

        for (_, stream) in connection.log_streams.lock().unwrap().drain() {
            stream.abort();
        }
        drop(connection);
        self.release(id).await;
        if tokio::time::timeout(LAST_MESSAGES_TIMEOUT, &mut writer).await.is_err() {
            writer.abort();
        }
    }

    /// Forgets a closed connection and stops the Runs nobody needs any more:
    /// none while an app is connected, else every Run that no remaining
    /// connection holds.
    async fn release(&self, id: u64) {
        let (held, origin) = {
            let mut connections = self.connections();
            connections.dismissers.remove(&id);
            let client = connections.clients.remove(&id).unwrap_or_default();
            for holders in connections.holders.values_mut() {
                holders.remove(&id);
            }
            connections.holders.retain(|_, holders| !holders.is_empty());
            if connections.clients.values().any(|client| *client == Client::App) {
                return;
            }
            (connections.holders.keys().copied().collect::<HashSet<Id>>(), client.origin())
        };
        if held.is_empty() {
            self.manager.stop_every_run(origin).await;
            return;
        }
        let (statuses, _) = self.manager.subscribe();
        let mut stops = tokio::task::JoinSet::new();
        for status in statuses.into_values() {
            if status.state.is_active() && !held.contains(&status.service_id) {
                let manager = self.manager.clone();
                stops.spawn(async move {
                    let _ = manager.stop(status.service_id, origin).await;
                });
            }
        }
        stops.join_all().await;
    }

    /// Records that `connection` needs `service_ids` and their Dependencies.
    fn hold(&self, connection: u64, service_ids: Vec<Id>, is_new_run: bool) {
        let service_ids = self.with_dependencies(service_ids);
        let mut connections = self.connections();
        // A request still running when its connection closed holds nothing.
        if !connections.clients.contains_key(&connection) {
            return;
        }
        for (index, service_id) in service_ids.into_iter().enumerate() {
            let holders = connections.holders.entry(service_id).or_default();
            // A restarted Service's new Run is its restarter's alone.
            if is_new_run && index == 0 {
                holders.clear();
            }
            holders.insert(connection);
        }
    }

    /// Stopped Runs are needed by nobody; the next start says who needs it.
    fn release_services(&self, service_ids: impl IntoIterator<Item = Id>) {
        let mut connections = self.connections();
        for service_id in service_ids {
            connections.holders.remove(&service_id);
        }
    }

    /// `service_ids`, first, then every Service they depend on, directly or not.
    fn with_dependencies(&self, mut service_ids: Vec<Id>) -> Vec<Id> {
        let mut index = 0;
        while index < service_ids.len() {
            if let Ok((_, service)) = self.library.service(service_ids[index]) {
                for dependency in service.dependencies {
                    if !service_ids.contains(&dependency) {
                        service_ids.push(dependency);
                    }
                }
            }
            index += 1;
        }
        service_ids
    }
}

async fn write_messages<W: AsyncWrite + Unpin>(mut receiver: mpsc::UnboundedReceiver<Message>, mut output: W) {
    while let Some(message) = receiver.recv().await {
        let mut line = serde_json::to_vec(&message).expect("a message always encodes");
        line.push(b'\n');
        // Ends when the app stops reading; the input then closes too.
        if output.write_all(&line).await.is_err() || output.flush().await.is_err() {
            return;
        }
    }
}

fn value(value: impl Serialize) -> Result<serde_json::Value, LocalhostError> {
    Ok(serde_json::to_value(value).expect("a reply always encodes"))
}

fn done() -> Result<serde_json::Value, LocalhostError> {
    Ok(serde_json::Value::Null)
}

impl Connection {
    fn client(&self) -> Client {
        self.core.connections().clients.get(&self.id).copied().unwrap_or_default()
    }

    async fn handle(self: &Arc<Self>, request: Request) -> Result<serde_json::Value, LocalhostError> {
        let core = &self.core;
        let manager = &core.manager;
        match request {
            Request::Subscribe => unreachable!("answered by `subscribe`"),
            Request::Identify { client } => {
                core.connections().clients.insert(self.id, client);
                done()
            }
            Request::SetProjects { projects, secrets } => {
                // Values a client didn't send are kept, for other clients'
                // Runs, until no Project names them.
                let named: HashSet<String> = projects.iter().flat_map(all_secret_accounts).collect();
                core.secrets.update(secrets, &named);
                core.library.replace_projects(projects, self.client().origin());
                done()
            }
            Request::Start { service_id, on_port, origin } => {
                // Held before it starts, so a disconnect meanwhile stops it.
                core.hold(self.id, vec![service_id], false);
                value(manager.start(service_id, on_port, origin).await?)
            }
            Request::Stop { service_id, origin } => {
                let status = manager.stop(service_id, origin).await?;
                core.release_services([service_id]);
                value(status)
            }
            Request::Restart { service_id, origin } => {
                core.hold(self.id, vec![service_id], true);
                value(manager.restart(service_id, origin).await?)
            }
            Request::StartAll { project_id, origin } => {
                let project = core.library.project(project_id)?;
                let included = project.services.iter().filter(|service| !service.is_excluded_from_start_all);
                core.hold(self.id, included.map(|service| service.id).collect(), false);
                value(manager.start_all(project_id, origin).await?)
            }
            Request::StopAll { project_id, origin } => {
                let project = core.library.project(project_id)?;
                manager.stop_all(project_id, origin).await?;
                core.release_services(project.services.iter().map(|service| service.id));
                done()
            }
            Request::StopEveryRun { origin } => {
                manager.stop_every_run(origin).await;
                core.connections().holders.clear();
                done()
            }
            Request::Retire => {
                core.is_retiring.store(true, Ordering::Relaxed);
                done()
            }
            Request::AgentCount => value(core.agents().len()),
            Request::DisconnectAgents => {
                let agents = core.agents();
                let connections = core.connections();
                for id in &agents {
                    if let Some((output, dismissed)) = connections.dismissers.get(id) {
                        let _ = output.send(Message::Dismissed);
                        dismissed.notify_one();
                    }
                }
                value(agents.len())
            }
            Request::SetAutomaticPortMove { is_enabled } => {
                // An app setting: an agent's or the command's default doesn't override it.
                if self.client() == Client::App {
                    manager.set_automatic_port_move(is_enabled);
                }
                done()
            }
            Request::Listeners => value(manager.listeners().await),
            Request::StopListener { listener, origin } => value(manager.stop_listener(&listener, origin).await?),
            Request::RunStats { service_id, previous } => value(manager.run_stats(service_id, previous).await?),
            Request::NextFreePort { service_id } => value(manager.next_free_port(service_id).await?),
            Request::Orphans => value(core.runs.orphans()),
            Request::StopOrphans { orphans } => {
                core.runs.stop_orphans(&orphans, core.stop_policy).await;
                done()
            }
            Request::LeaveOrphans { orphans } => {
                core.runs.leave_orphans(&orphans);
                done()
            }
            Request::RemoveLogs { service_id } => {
                core.logs.remove_logs(service_id);
                done()
            }
            Request::ClearBuffer { service_id } => {
                core.logs.clear_buffer(service_id);
                done()
            }
            Request::OpenRunLog { stream, run_id } => {
                let (lines, receiver) = core.logs.subscribe_to_run(run_id);
                let output = self.output.clone();
                self.open_stream(
                    stream,
                    forward_batches(receiver, move |lines, is_finished| {
                        output.send(Message::RunLog { stream, lines, is_finished }).is_ok()
                    }),
                );
                value(lines)
            }
            Request::OpenServicesLog { stream, service_ids } => {
                let (lines, receiver) = core.logs.subscribe_to_services(service_ids);
                let output = self.output.clone();
                self.open_stream(
                    stream,
                    forward_batches(receiver, move |lines, _| {
                        output.send(Message::ServicesLog { stream, lines }).is_ok()
                    }),
                );
                value(lines)
            }
            Request::CloseLog { stream } => {
                if let Some(stream) = self.log_streams.lock().unwrap().remove(&stream) {
                    stream.abort();
                }
                done()
            }
            Request::ScanFolder { path } => {
                let scan = tokio::task::spawn_blocking(move || ProjectScanner::default().scan_folder(&path));
                value(scan.await.unwrap_or_default())
            }
            Request::ScanProject { project } => {
                let scan = tokio::task::spawn_blocking(move || ProjectScanner::default().scan_project(&project));
                value(scan.await.unwrap_or_default())
            }
        }
    }

    /// Replies with the statuses, then sends every event after them.
    fn subscribe(self: &Arc<Self>, id: u64) {
        let mut events = self.events.lock().unwrap();
        let (statuses, receiver) = self.core.manager.subscribe();
        let _ = self.output.send(Message::Reply { id, result: value(statuses) });
        *events = Some(receiver);
        drop(events);
        let connection = Arc::downgrade(self);
        tokio::spawn(std::future::poll_fn(move |task| {
            let Some(connection) = connection.upgrade() else { return Poll::Ready(()) };
            let mut events = connection.events.lock().unwrap();
            let Some(receiver) = events.as_mut() else { return Poll::Ready(()) };
            // Sent while holding `events`, so no reply overtakes them.
            while let Poll::Ready(event) = receiver.poll_recv(task) {
                let Some(event) = event else { return Poll::Ready(()) };
                if connection.output.send(Message::Event { event }).is_err() {
                    return Poll::Ready(());
                }
            }
            Poll::Pending
        }));
    }

    /// Sends the events published so far, then the reply.
    fn reply(&self, id: u64, result: Result<serde_json::Value, LocalhostError>) {
        let mut events = self.events.lock().unwrap();
        if let Some(receiver) = events.as_mut() {
            while let Ok(event) = receiver.try_recv() {
                let _ = self.output.send(Message::Event { event });
            }
        }
        let _ = self.output.send(Message::Reply { id, result });
    }

    fn open_stream(self: &Arc<Self>, stream: u64, forward: impl Future<Output = ()> + Send + 'static) {
        let connection = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            forward.await;
            if let Some(connection) = connection.upgrade() {
                connection.log_streams.lock().unwrap().remove(&stream);
            }
        });
        if let Some(previous) = self.log_streams.lock().unwrap().insert(stream, task) {
            previous.abort();
        }
    }
}

/// Sends what `receiver` delivers in batches, at most one per
/// `LOG_BATCH_INTERVAL`, until it ends (the last batch says so) or `send`
/// returns false.
async fn forward_batches<T>(mut receiver: mpsc::UnboundedReceiver<T>, send: impl Fn(Vec<T>, bool) -> bool) {
    loop {
        let Some(first) = receiver.recv().await else {
            send(Vec::new(), true);
            return;
        };
        tokio::time::sleep(LOG_BATCH_INTERVAL).await;
        let mut batch = vec![first];
        let mut has_ended = false;
        loop {
            match receiver.try_recv() {
                Ok(item) => batch.push(item),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    has_ended = true;
                    break;
                }
            }
        }
        if !send(batch, has_ended) || has_ended {
            return;
        }
    }
}
