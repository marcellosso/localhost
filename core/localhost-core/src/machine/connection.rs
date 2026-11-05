//! The viewing app's side of the Machine protocol: another Machine's
//! Projects, statuses, controls and logs, over any byte stream.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

use super::MachineProblem;
use super::protocol::{MachineCall, MachineHello, MachineMessage, MachineRequest};
use crate::{Id, LocalhostError, OutputLine, Project, ServiceEvent, ServiceOutputLine, ServiceStatus};

/// How often the app tells the other end it is still there, well within
/// `relay::SILENCE_TIMEOUT`, and how long an answer may take before the
/// connection counts as lost: a laptop that slept, a network that changed.
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const PING_TIMEOUT: Duration = Duration::from_secs(20);

/// A connection to another Machine. Cheap to clone; every clone is the same
/// connection. Once it is lost, calls fail with `MachineUnavailable`, the
/// streams end, and `closed` returns. Must be used inside a tokio runtime.
#[derive(Clone)]
pub struct MachineConnection {
    inner: Arc<Inner>,
}

struct Inner {
    machine: String,
    hello: MachineHello,
    outgoing: mpsc::UnboundedSender<String>,
    /// Ends the writer, so the other end sees its input close.
    stop_writing: Arc<tokio::sync::Notify>,
    next_id: AtomicU64,
    state: Mutex<State>,
    closed: watch::Sender<bool>,
}

#[derive(Default)]
struct State {
    is_connected: bool,
    pending: HashMap<u64, oneshot::Sender<Result<serde_json::Value, LocalhostError>>>,
    projects: Vec<mpsc::UnboundedSender<Vec<Project>>>,
    events: Vec<mpsc::UnboundedSender<ServiceEvent>>,
    run_logs: HashMap<u64, mpsc::UnboundedSender<OutputLine>>,
    services_logs: HashMap<u64, mpsc::UnboundedSender<ServiceOutputLine>>,
}

impl MachineConnection {
    /// Connects over `reader` and `writer` to the Machine called `machine`:
    /// reads its hello, then pings it every `PING_INTERVAL`.
    pub async fn connect<R, W>(machine: &str, reader: R, mut writer: W) -> Result<MachineConnection, LocalhostError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let unavailable = |problem| LocalhostError::MachineUnavailable { machine: machine.to_string(), problem };
        let mut lines = BufReader::new(reader).lines();
        let hello = loop {
            match lines.next_line().await {
                Ok(Some(line)) => match serde_json::from_str::<MachineMessage>(&line) {
                    Ok(MachineMessage::Hello(hello)) => break hello,
                    Ok(MachineMessage::Refused { reason }) => {
                        return Err(unavailable(MachineProblem::CantServe(reason)));
                    }
                    // A login banner or a profile's echo on standard output.
                    _ => continue,
                },
                Ok(None) | Err(_) => return Err(unavailable(MachineProblem::Lost)),
            }
        };

        let (outgoing, mut queue) = mpsc::unbounded_channel::<String>();
        let stop_writing = Arc::new(tokio::sync::Notify::new());
        let stop = stop_writing.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = queue.recv() => {
                        let Some(line) = line else { break };
                        if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                            return;
                        }
                    }
                    _ = stop.notified() => break,
                }
            }
            let _ = writer.shutdown().await;
        });
        let (closed, _) = watch::channel(false);
        let connection = MachineConnection {
            inner: Arc::new(Inner {
                machine: machine.to_string(),
                hello,
                outgoing,
                stop_writing,
                next_id: AtomicU64::new(1),
                state: Mutex::new(State { is_connected: true, ..State::default() }),
                closed,
            }),
        };

        let weak = Arc::downgrade(&connection.inner);
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let Some(inner) = weak.upgrade() else { return };
                MachineConnection { inner }.receive(&line);
            }
            if let Some(inner) = weak.upgrade() {
                MachineConnection { inner }.disconnect();
            }
        });

        let weak = Arc::downgrade(&connection.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PING_INTERVAL).await;
                let Some(inner) = weak.upgrade() else { return };
                let connection = MachineConnection { inner };
                if !connection.is_connected() {
                    return;
                }
                let ping = connection.call::<()>(MachineRequest::Ping);
                if !matches!(tokio::time::timeout(PING_TIMEOUT, ping).await, Ok(Ok(()))) {
                    connection.disconnect();
                    return;
                }
            }
        });
        Ok(connection)
    }

    pub fn hello(&self) -> &MachineHello {
        &self.inner.hello
    }

    pub fn is_connected(&self) -> bool {
        self.state().is_connected
    }

    /// Returns once the connection is lost.
    pub async fn closed(&self) {
        let mut closed = self.inner.closed.subscribe();
        let _ = closed.wait_for(|closed| *closed).await;
    }

    /// Ends the connection, as if it were lost.
    pub fn close(&self) {
        self.disconnect();
    }

    /// The other Machine's Projects, then every Project after each change.
    pub async fn projects(&self) -> Result<(Vec<Project>, mpsc::UnboundedReceiver<Vec<Project>>), LocalhostError> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state().projects.push(sender);
        Ok((self.call(MachineRequest::Projects).await?, receiver))
    }

    /// Every status there, then every Service event after them.
    pub async fn subscribe(
        &self,
    ) -> Result<(HashMap<Id, ServiceStatus>, mpsc::UnboundedReceiver<ServiceEvent>), LocalhostError> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state().events.push(sender);
        Ok((self.call(MachineRequest::Subscribe).await?, receiver))
    }

    pub async fn start(&self, service_id: Id, on_port: Option<u16>) -> Result<ServiceStatus, LocalhostError> {
        self.call(MachineRequest::Start { service_id, on_port }).await
    }

    pub async fn stop(&self, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        self.call(MachineRequest::Stop { service_id }).await
    }

    pub async fn restart(&self, service_id: Id) -> Result<ServiceStatus, LocalhostError> {
        self.call(MachineRequest::Restart { service_id }).await
    }

    pub async fn start_all(&self, project_id: Id) -> Result<(), LocalhostError> {
        self.call(MachineRequest::StartAll { project_id }).await
    }

    pub async fn stop_all(&self, project_id: Id) -> Result<(), LocalhostError> {
        self.call(MachineRequest::StopAll { project_id }).await
    }

    /// A Run's buffered lines, then every line after them until it ends.
    pub async fn subscribe_to_run(
        &self,
        run_id: Id,
    ) -> Result<(Vec<OutputLine>, mpsc::UnboundedReceiver<OutputLine>), LocalhostError> {
        let stream = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state().run_logs.insert(stream, sender);
        let lines = self.call(MachineRequest::OpenRunLog { stream, run_id }).await.inspect_err(|_| {
            self.state().run_logs.remove(&stream);
        })?;
        self.close_when_dropped(stream);
        Ok((lines, receiver))
    }

    /// Several Services' lines, buffered then live, as one stream.
    pub async fn subscribe_to_services(
        &self,
        service_ids: HashSet<Id>,
    ) -> Result<(Vec<ServiceOutputLine>, mpsc::UnboundedReceiver<ServiceOutputLine>), LocalhostError> {
        let stream = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.state().services_logs.insert(stream, sender);
        let lines = self.call(MachineRequest::OpenServicesLog { stream, service_ids }).await.inspect_err(|_| {
            self.state().services_logs.remove(&stream);
        })?;
        self.close_when_dropped(stream);
        Ok((lines, receiver))
    }

    // MARK: Plumbing

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn unavailable(&self, problem: MachineProblem) -> LocalhostError {
        LocalhostError::MachineUnavailable { machine: self.inner.machine.clone(), problem }
    }

    async fn call<T: DeserializeOwned>(&self, request: MachineRequest) -> Result<T, LocalhostError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        {
            let mut state = self.state();
            if !state.is_connected {
                return Err(self.unavailable(MachineProblem::Lost));
            }
            state.pending.insert(id, sender);
        }
        let mut line = serde_json::to_string(&MachineCall { id, request }).expect("a request always encodes");
        line.push('\n');
        if self.inner.outgoing.send(line).is_err() {
            self.state().pending.remove(&id);
            return Err(self.unavailable(MachineProblem::Lost));
        }
        let value = receiver.await.map_err(|_| self.unavailable(MachineProblem::Lost))??;
        serde_json::from_value(value).map_err(|_| LocalhostError::Unsupported)
    }

    fn receive(&self, line: &str) {
        let message = match serde_json::from_str::<MachineMessage>(line) {
            Ok(message) => message,
            Err(error) => {
                // A reply this version can't read, e.g. an error added later:
                // its call fails rather than waiting forever.
                if let Some(id) = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|value| value.get("id").and_then(serde_json::Value::as_u64))
                    && let Some(pending) = self.state().pending.remove(&id)
                {
                    let _ = pending.send(Err(LocalhostError::Unsupported));
                }
                eprintln!("localhost: ignoring a message from {} it can't read: {error}", self.inner.machine);
                return;
            }
        };
        let mut state = self.state();
        match message {
            MachineMessage::Hello(_) | MachineMessage::Refused { .. } | MachineMessage::Unknown => {}
            MachineMessage::Reply { id, result } => {
                if let Some(pending) = state.pending.remove(&id) {
                    let _ = pending.send(result);
                }
            }
            MachineMessage::Projects { projects } => {
                state.projects.retain(|subscriber| subscriber.send(projects.clone()).is_ok());
            }
            MachineMessage::Event { event } => {
                state.events.retain(|subscriber| subscriber.send(event.clone()).is_ok());
            }
            MachineMessage::RunLog { stream, lines, is_finished } => {
                if let Some(sender) = state.run_logs.get(&stream) {
                    let is_open = lines.into_iter().all(|line| sender.send(line).is_ok());
                    if is_finished || !is_open {
                        state.run_logs.remove(&stream);
                    }
                }
            }
            MachineMessage::ServicesLog { stream, lines } => {
                if let Some(sender) = state.services_logs.get(&stream)
                    && !lines.into_iter().all(|line| sender.send(line).is_ok())
                {
                    state.services_logs.remove(&stream);
                }
            }
        }
    }

    /// Fails every pending call and ends every stream.
    fn disconnect(&self) {
        let pending = {
            let mut state = self.state();
            if !state.is_connected {
                return;
            }
            let pending: Vec<_> = state.pending.drain().map(|(_, sender)| sender).collect();
            *state = State::default();
            pending
        };
        for sender in pending {
            let _ = sender.send(Err(self.unavailable(MachineProblem::Lost)));
        }
        self.inner.stop_writing.notify_one();
        self.inner.closed.send_replace(true);
    }

    /// Tells the other end to stop a log stream once its receiver is dropped.
    fn close_when_dropped(&self, stream: u64) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let Some(inner) = weak.upgrade() else { return };
                let connection = MachineConnection { inner };
                let is_gone = {
                    let state = connection.state();
                    !state.is_connected
                        || match (state.run_logs.get(&stream), state.services_logs.get(&stream)) {
                            (Some(sender), _) => sender.is_closed(),
                            (_, Some(sender)) => sender.is_closed(),
                            (None, None) => true,
                        }
                };
                if is_gone {
                    {
                        let mut state = connection.state();
                        state.run_logs.remove(&stream);
                        state.services_logs.remove(&stream);
                    }
                    if connection.is_connected() {
                        let _ = connection.call::<()>(MachineRequest::CloseLog { stream }).await;
                    }
                    return;
                }
            }
        });
    }
}
