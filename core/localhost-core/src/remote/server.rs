//! `localhostd`'s side: a Service manager, logs and Run records on this
//! machine, driven by the requests on its input.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{Call, Hello, Message, Request};
use crate::{
    LocalhostError, LogStore, MemorySecretStore, Origin, ProjectLibrary, ProjectScanner, RunRegistry, ServiceEvent,
    ServiceManager, ServiceManagerOptions, StopPolicy,
};

/// How often log lines cross, at most: once per frame.
const LOG_BATCH_INTERVAL: Duration = Duration::from_millis(16);

struct Context {
    secrets: Arc<MemorySecretStore>,
    library: Arc<ProjectLibrary>,
    logs: Arc<LogStore>,
    runs: Arc<RunRegistry>,
    manager: ServiceManager,
    stop_policy: StopPolicy,
    output: mpsc::UnboundedSender<Message>,
    /// The Service manager's events not sent yet. Every reply sends them
    /// first, so the app has a call's events by the time it has its reply.
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
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_messages(receiver, output));
    let _ = sender.send(Message::Hello(Hello::current()));

    let secrets = Arc::new(MemorySecretStore::new());
    let library = Arc::new(ProjectLibrary::mirror(secrets.clone()));
    let logs = Arc::new(LogStore::new(storage_root));
    let runs = Arc::new(RunRegistry::new(storage_root));
    let stop_policy = options.stop_policy;
    let manager = ServiceManager::new(library.clone(), logs.clone(), runs.clone(), options);
    let context = Arc::new(Context {
        secrets,
        library,
        logs,
        runs,
        manager,
        stop_policy,
        output: sender,
        events: Mutex::new(None),
        log_streams: Mutex::new(HashMap::new()),
    });

    let mut lines = BufReader::new(input).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<Call>(&line) {
            Ok(Call { id, request }) => {
                let context = context.clone();
                tokio::spawn(async move {
                    if let Request::Subscribe = request {
                        context.subscribe(id);
                    } else {
                        let result = context.handle(request).await;
                        context.reply(id, result);
                    }
                });
            }
            Err(error) => eprintln!("localhostd: ignoring a request it can't read: {error}"),
        }
    }

    // The app is gone: nothing may outlive it, as when it quits.
    for (_, stream) in context.log_streams.lock().unwrap().drain() {
        stream.abort();
    }
    context.manager.stop_every_run(Origin::MainWindow).await;
    drop(context);
    let _ = writer.await;
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

impl Context {
    async fn handle(self: &Arc<Self>, request: Request) -> Result<serde_json::Value, LocalhostError> {
        let manager = &self.manager;
        match request {
            Request::Subscribe => unreachable!("answered by `subscribe`"),
            Request::SetProjects { projects, secrets } => {
                self.secrets.replace_all(secrets);
                self.library.replace_projects(projects, Origin::MainWindow);
                done()
            }
            Request::Start { service_id, on_port, origin } => value(manager.start(service_id, on_port, origin).await?),
            Request::Stop { service_id, origin } => value(manager.stop(service_id, origin).await?),
            Request::Restart { service_id, origin } => value(manager.restart(service_id, origin).await?),
            Request::StartAll { project_id, origin } => value(manager.start_all(project_id, origin).await?),
            Request::StopAll { project_id, origin } => value(manager.stop_all(project_id, origin).await?),
            Request::StopEveryRun { origin } => {
                manager.stop_every_run(origin).await;
                done()
            }
            Request::SetAutomaticPortMove { is_enabled } => {
                manager.set_automatic_port_move(is_enabled);
                done()
            }
            Request::Listeners => value(manager.listeners().await),
            Request::StopListener { listener, origin } => value(manager.stop_listener(&listener, origin).await?),
            Request::RunStats { service_id, previous } => value(manager.run_stats(service_id, previous).await?),
            Request::NextFreePort { service_id } => value(manager.next_free_port(service_id).await?),
            Request::Orphans => value(self.runs.orphans()),
            Request::StopOrphans { orphans } => {
                self.runs.stop_orphans(&orphans, self.stop_policy).await;
                done()
            }
            Request::LeaveOrphans { orphans } => {
                self.runs.leave_orphans(&orphans);
                done()
            }
            Request::RemoveLogs { service_id } => {
                self.logs.remove_logs(service_id);
                done()
            }
            Request::ClearBuffer { service_id } => {
                self.logs.clear_buffer(service_id);
                done()
            }
            Request::OpenRunLog { stream, run_id } => {
                let (lines, receiver) = self.logs.subscribe_to_run(run_id);
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
                let (lines, receiver) = self.logs.subscribe_to_services(service_ids);
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
        let (statuses, receiver) = self.manager.subscribe();
        let _ = self.output.send(Message::Reply { id, result: value(statuses) });
        *events = Some(receiver);
        drop(events);
        let context = Arc::downgrade(self);
        tokio::spawn(std::future::poll_fn(move |task| {
            let Some(context) = context.upgrade() else { return Poll::Ready(()) };
            let mut events = context.events.lock().unwrap();
            let Some(receiver) = events.as_mut() else { return Poll::Ready(()) };
            // Sent while holding `events`, so no reply overtakes them.
            while let Poll::Ready(event) = receiver.poll_recv(task) {
                let Some(event) = event else { return Poll::Ready(()) };
                if context.output.send(Message::Event { event }).is_err() {
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
        let context = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            forward.await;
            if let Some(context) = context.upgrade() {
                context.log_streams.lock().unwrap().remove(&stream);
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
