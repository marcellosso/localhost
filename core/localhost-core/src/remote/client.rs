//! The app's side: a localhost core somewhere else, with the Service
//! manager's API.

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use super::{Call, Client, Hello, Message, PROTOCOL_VERSION, Request};
use crate::{
    Id, Listener, LocalhostError, Origin, Orphan, OutputLine, Project, RunStats, ServiceEvent, ServiceOutputLine,
    ServiceStatus, ServiceSuggestion,
};

/// A connection to `localhostd`. Cheap to clone; every clone is the same
/// connection. Once it is lost, every call fails with `TargetUnavailable`,
/// event and log streams end, and `closed` returns.
///
/// Must be used inside a tokio runtime.
#[derive(Clone)]
pub struct Remote {
    inner: Arc<Inner>,
}

struct Inner {
    hello: Hello,
    /// Builds the error for a lost connection: `TargetUnavailable`, or
    /// `CoreUnavailable` for the shared core on this machine.
    unavailable: fn(String) -> LocalhostError,
    outgoing: mpsc::UnboundedSender<String>,
    next_id: AtomicU64,
    state: Mutex<State>,
    closed: tokio::sync::watch::Receiver<bool>,
    /// The process at the other end, if `spawn` started it. Not killed: when
    /// the last clone is dropped its input closes, and it stops its Runs
    /// and exits on its own.
    _child: Option<tokio::process::Child>,
}

#[derive(Default)]
struct State {
    is_connected: bool,
    /// The core said it is closing this connection on purpose (`Message::Dismissed`).
    is_dismissed: bool,
    pending: HashMap<u64, oneshot::Sender<Result<serde_json::Value, LocalhostError>>>,
    /// Every Service's status as `localhostd` last reported it.
    statuses: HashMap<Id, ServiceStatus>,
    subscribers: Vec<mpsc::UnboundedSender<ServiceEvent>>,
    run_logs: HashMap<u64, mpsc::UnboundedSender<OutputLine>>,
    services_logs: HashMap<u64, mpsc::UnboundedSender<ServiceOutputLine>>,
}

const LOST: &str = "the connection to localhostd closed";

impl Remote {
    /// Starts `command` (e.g. `wsl.exe -d Ubuntu -- ~/.local/share/localhost/localhostd --stdio`)
    /// and connects to it over its standard input and output.
    pub async fn spawn(mut command: tokio::process::Command) -> Result<Remote, LocalhostError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command.spawn().map_err(|error| LocalhostError::TargetUnavailable(error.to_string()))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(LocalhostError::TargetUnavailable("its input and output aren't available".into()));
        };
        Remote::connect_owning(output, input, Some(child), LocalhostError::TargetUnavailable).await
    }

    /// Connects over `reader` and `writer`: reads the `Hello`, then the
    /// statuses. Fails if the other end speaks another protocol version.
    pub async fn connect<R, W>(reader: R, writer: W) -> Result<Remote, LocalhostError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Remote::connect_owning(reader, writer, None, LocalhostError::TargetUnavailable).await
    }

    pub(crate) async fn connect_owning<R, W>(
        reader: R,
        mut writer: W,
        child: Option<tokio::process::Child>,
        unavailable: fn(String) -> LocalhostError,
    ) -> Result<Remote, LocalhostError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let mut lines = BufReader::new(reader).lines();
        let first = lines.next_line().await.map_err(|error| unavailable(error.to_string()))?;
        let hello = match first.as_deref().map(serde_json::from_str::<Message>) {
            Some(Ok(Message::Hello(hello))) => hello,
            None => return Err(unavailable("it ended before saying hello".into())),
            Some(_) => return Err(unavailable("it didn't say hello".into())),
        };
        if hello.protocol != PROTOCOL_VERSION {
            return Err(unavailable(format!(
                "localhostd {} speaks protocol {}, this app {PROTOCOL_VERSION}",
                hello.version, hello.protocol
            )));
        }

        let (outgoing, mut queue) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(line) = queue.recv().await {
                if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                    return;
                }
            }
            // Every clone is gone: closing the input tells localhostd to stop.
            let _ = writer.shutdown().await;
        });
        let (closed_sender, closed) = tokio::sync::watch::channel(false);
        let remote = Remote {
            inner: Arc::new(Inner {
                hello,
                unavailable,
                outgoing,
                next_id: AtomicU64::new(1),
                state: Mutex::new(State { is_connected: true, ..State::default() }),
                closed,
                _child: child,
            }),
        };
        let weak = Arc::downgrade(&remote.inner);
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(inner) = weak.upgrade() else { return };
                match serde_json::from_str::<Message>(&line) {
                    Ok(message) => Remote { inner }.receive(message),
                    Err(error) => eprintln!("localhost: ignoring a message from localhostd it can't read: {error}"),
                }
            }
            if let Some(inner) = weak.upgrade() {
                Remote { inner }.disconnect();
            }
            closed_sender.send_replace(true);
        });

        let statuses: HashMap<Id, ServiceStatus> = remote.call(Request::Subscribe).await?;
        remote.state().statuses.extend(statuses);
        Ok(remote)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn receive(&self, message: Message) {
        let mut state = self.state();
        match message {
            Message::Hello(_) => {}
            Message::Reply { id, result } => {
                if let Some(reply) = state.pending.remove(&id) {
                    let _ = reply.send(result);
                }
            }
            Message::Event { event } => {
                if let ServiceEvent::ServiceStateChanged(status) = &event {
                    state.statuses.insert(status.service_id, status.clone());
                }
                state.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
            }
            Message::RunLog { stream, lines, is_finished } => {
                let Some(sender) = state.run_logs.get(&stream) else { return };
                let is_read = lines.into_iter().all(|line| sender.send(line).is_ok());
                if !is_read || is_finished {
                    state.run_logs.remove(&stream);
                    drop(state);
                    self.close_stream(stream, is_read && is_finished);
                }
            }
            Message::ServicesLog { stream, lines } => {
                let Some(sender) = state.services_logs.get(&stream) else { return };
                if !lines.into_iter().all(|line| sender.send(line).is_ok()) {
                    state.services_logs.remove(&stream);
                    drop(state);
                    self.close_stream(stream, false);
                }
            }
            Message::Dismissed => state.is_dismissed = true,
        }
    }

    /// Stops `localhostd` sending a stream nobody reads (unless it ended there).
    fn close_stream(&self, stream: u64, has_ended: bool) {
        if !has_ended {
            let remote = self.clone();
            tokio::spawn(async move {
                let _ = remote.call::<()>(Request::CloseLog { stream }).await;
            });
        }
    }

    fn disconnect(&self) {
        let mut state = self.state();
        state.is_connected = false;
        for (_, reply) in state.pending.drain() {
            let _ = reply.send(Err((self.inner.unavailable)(LOST.into())));
        }
        state.subscribers.clear();
        state.run_logs.clear();
        state.services_logs.clear();
    }

    async fn call<T: DeserializeOwned>(&self, request: Request) -> Result<T, LocalhostError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        {
            let mut state = self.state();
            if !state.is_connected {
                return Err((self.inner.unavailable)(LOST.into()));
            }
            state.pending.insert(id, sender);
        }
        let mut line = serde_json::to_string(&Call { id, request }).expect("a request always encodes");
        line.push('\n');
        if self.inner.outgoing.send(line).is_err() {
            self.state().pending.remove(&id);
            return Err((self.inner.unavailable)(LOST.into()));
        }
        let value = receiver.await.map_err(|_| (self.inner.unavailable)(LOST.into()))??;
        serde_json::from_value(value)
            .map_err(|error| (self.inner.unavailable)(format!("localhostd replied unexpectedly: {error}")))
    }

    // MARK: Connection

    pub fn hello(&self) -> &Hello {
        &self.inner.hello
    }

    pub fn is_connected(&self) -> bool {
        self.state().is_connected
    }

    /// Whether the core closed this connection on purpose, before an update:
    /// the client should end rather than reconnect.
    pub fn is_dismissed(&self) -> bool {
        self.state().is_dismissed
    }

    /// Returns once the connection is lost.
    pub async fn closed(&self) {
        let mut closed = self.inner.closed.clone();
        let _ = closed.wait_for(|closed| *closed).await;
    }

    /// Tells the core what this connection is, which decides what its
    /// disconnecting stops (`Client`).
    pub async fn identify(&self, client: Client) -> Result<(), LocalhostError> {
        self.call(Request::Identify { client }).await
    }

    // MARK: Projects

    /// Makes `projects` the Projects `localhostd` runs, and sets the values
    /// of these Secrets by account (`ProjectLibrary::secrets_for_runs`); it
    /// keeps the others it has that a Project still names. `localhostd`
    /// keeps both in memory only.
    pub async fn set_projects(
        &self,
        projects: Vec<Project>,
        secrets: HashMap<String, Option<String>>,
    ) -> Result<(), LocalhostError> {
        self.call(Request::SetProjects { projects, secrets }).await
    }

    // MARK: Services, as `ServiceManager`

    pub fn status(&self, service_id: Id) -> ServiceStatus {
        self.state().statuses.get(&service_id).cloned().unwrap_or_else(|| ServiceStatus::never_run(service_id))
    }

    /// The status of every Service that has run, plus a stream of every
    /// change after that snapshot. The stream ends if the connection is lost.
    pub fn subscribe(&self) -> (HashMap<Id, ServiceStatus>, mpsc::UnboundedReceiver<ServiceEvent>) {
        let mut state = self.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        if state.is_connected {
            state.subscribers.push(sender);
        }
        (state.statuses.clone(), receiver)
    }

    pub async fn start(
        &self,
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    ) -> Result<ServiceStatus, LocalhostError> {
        self.call(Request::Start { service_id, on_port, origin }).await
    }

    pub async fn stop(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        self.call(Request::Stop { service_id, origin }).await
    }

    pub async fn restart(&self, service_id: Id, origin: Origin) -> Result<ServiceStatus, LocalhostError> {
        self.call(Request::Restart { service_id, origin }).await
    }

    pub async fn start_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        self.call(Request::StartAll { project_id, origin }).await
    }

    pub async fn stop_all(&self, project_id: Id, origin: Origin) -> Result<(), LocalhostError> {
        self.call(Request::StopAll { project_id, origin }).await
    }

    pub async fn stop_every_run(&self, origin: Origin) -> Result<(), LocalhostError> {
        self.call(Request::StopEveryRun { origin }).await
    }

    /// Asks the shared core to exit as soon as no client is connected, e.g.
    /// before the app quits to install an update.
    pub async fn retire(&self) -> Result<(), LocalhostError> {
        self.call(Request::Retire).await
    }

    /// How many coding agents are connected to this core.
    pub async fn agent_count(&self) -> Result<usize, LocalhostError> {
        self.call(Request::AgentCount).await
    }

    /// Tells every connected coding agent to end; returns how many there were.
    pub async fn disconnect_agents(&self) -> Result<usize, LocalhostError> {
        self.call(Request::DisconnectAgents).await
    }

    pub async fn set_automatic_port_move(&self, is_enabled: bool) -> Result<(), LocalhostError> {
        self.call(Request::SetAutomaticPortMove { is_enabled }).await
    }

    /// Every Listener where `localhostd` runs.
    pub async fn listeners(&self) -> Result<Vec<Listener>, LocalhostError> {
        self.call(Request::Listeners).await
    }

    pub async fn stop_listener(&self, listener: &Listener, origin: Origin) -> Result<(), LocalhostError> {
        self.call(Request::StopListener { listener: listener.clone(), origin }).await
    }

    pub async fn next_free_port(&self, service_id: Id) -> Result<Option<u16>, LocalhostError> {
        self.call(Request::NextFreePort { service_id }).await
    }

    pub async fn run_stats(
        &self,
        service_id: Id,
        previous: Option<RunStats>,
    ) -> Result<Option<RunStats>, LocalhostError> {
        self.call(Request::RunStats { service_id, previous }).await
    }

    // MARK: Orphans, as `RunRegistry`

    pub async fn orphans(&self) -> Result<Vec<Orphan>, LocalhostError> {
        self.call(Request::Orphans).await
    }

    pub async fn stop_orphans(&self, orphans: &[Orphan]) -> Result<(), LocalhostError> {
        self.call(Request::StopOrphans { orphans: orphans.to_vec() }).await
    }

    pub async fn leave_orphans(&self, orphans: &[Orphan]) -> Result<(), LocalhostError> {
        self.call(Request::LeaveOrphans { orphans: orphans.to_vec() }).await
    }

    // MARK: Logs, as `LogStore`

    pub async fn remove_logs(&self, service_id: Id) -> Result<(), LocalhostError> {
        self.call(Request::RemoveLogs { service_id }).await
    }

    /// As `LogStore::clear_buffer`: the log files are kept.
    pub async fn clear_buffer(&self, service_id: Id) -> Result<(), LocalhostError> {
        self.call(Request::ClearBuffer { service_id }).await
    }

    /// The buffered lines of a Run plus a stream of every line after them.
    /// The stream ends when the Run's output ends; dropping it stops it.
    pub async fn subscribe_to_run(
        &self,
        run_id: Id,
    ) -> Result<(Vec<OutputLine>, mpsc::UnboundedReceiver<OutputLine>), LocalhostError> {
        let stream = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        // Registered first, so no batch arrives before there's a place for it.
        self.state().run_logs.insert(stream, sender);
        match self.call(Request::OpenRunLog { stream, run_id }).await {
            Ok(lines) => Ok((lines, receiver)),
            Err(error) => {
                self.state().run_logs.remove(&stream);
                Err(error)
            }
        }
    }

    /// The combined stream of several Services, as `LogStore::subscribe_to_services`.
    pub async fn subscribe_to_services(
        &self,
        service_ids: HashSet<Id>,
    ) -> Result<(Vec<ServiceOutputLine>, mpsc::UnboundedReceiver<ServiceOutputLine>), LocalhostError> {
        let stream = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state().services_logs.insert(stream, sender);
        match self.call(Request::OpenServicesLog { stream, service_ids }).await {
            Ok(lines) => Ok((lines, receiver)),
            Err(error) => {
                self.state().services_logs.remove(&stream);
                Err(error)
            }
        }
    }

    // MARK: Detection

    /// Suggestions for a folder where `localhostd` runs, as `ProjectScanner::scan_folder`.
    pub async fn scan_folder(&self, path: &str) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        self.call(Request::ScanFolder { path: path.to_string() }).await
    }

    /// As `ProjectScanner::scan_project`.
    pub async fn scan_project(&self, project: &Project) -> Result<Vec<ServiceSuggestion>, LocalhostError> {
        self.call(Request::ScanProject { project: project.clone() }).await
    }
}
