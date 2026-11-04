//! The other Machine's side: `localhost remote --stdio`, which an app runs
//! over SSH. It is one more client of this Machine's shared core, as
//! `localhost mcp` is, with this Machine's own `ProjectLibrary` and secret
//! store: the app sees the Projects saved here, and a start reads the
//! Secret values here. No Secret value is ever written to the app.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::protocol::{MachineCall, MachineHello, MachineMessage, MachineRequest};
use crate::remote::forward_batches;
use crate::{LocalhostError, Origin, ProjectLibrary, Targets};

/// How long the app may stay silent before the connection counts as lost.
/// The app pings far more often; a laptop that went to sleep never closes
/// its SSH connection, and its Runs here would otherwise be held forever.
pub const SILENCE_TIMEOUT: Duration = Duration::from_secs(45);

const ORIGIN: Origin = Origin::Machine;

/// Answers the app on `input` and `output` until it closes or goes silent
/// for `silence_timeout`. `targets` reaches this Machine's shared core as
/// `Client::Machine`, so closing it lets the core stop what only this
/// connection started.
pub async fn serve<R, W>(input: R, output: W, library: Arc<ProjectLibrary>, targets: Targets, silence_timeout: Duration)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_messages(receiver, output));
    let _ = sender.send(MachineMessage::Hello(MachineHello::current()));
    let relay = Arc::new(Relay { library, targets, output: sender, streams: Mutex::new(HashMap::new()) });

    let mut lines = BufReader::new(input).lines();
    let mut forwarders = Vec::new();
    loop {
        let line = match tokio::time::timeout(silence_timeout, lines.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Ok(_) => break,
            Err(_) => {
                eprintln!("localhost: the app was silent for {silence_timeout:?}, disconnecting");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let call = match serde_json::from_str::<MachineCall>(&line) {
            Ok(call) => call,
            Err(error) => {
                eprintln!("localhost: ignoring a request it can't read: {error}");
                continue;
            }
        };
        match call.request {
            MachineRequest::Projects => forwarders.push(relay.forward_projects(call.id)),
            MachineRequest::Subscribe => forwarders.push(relay.forward_events(call.id)),
            request => {
                let relay = relay.clone();
                tokio::spawn(async move {
                    let result = relay.handle(request).await;
                    let _ = relay.output.send(MachineMessage::Reply { id: call.id, result });
                });
            }
        }
    }

    for forwarder in forwarders {
        forwarder.abort();
    }
    for (_, stream) in relay.streams.lock().unwrap().drain() {
        stream.abort();
    }
    drop(relay);
    let _ = tokio::time::timeout(Duration::from_secs(1), writer).await;
}

struct Relay {
    library: Arc<ProjectLibrary>,
    targets: Targets,
    output: mpsc::UnboundedSender<MachineMessage>,
    streams: Mutex<HashMap<u64, JoinHandle<()>>>,
}

fn value(value: impl Serialize) -> Result<serde_json::Value, LocalhostError> {
    Ok(serde_json::to_value(value).expect("a reply always encodes"))
}

fn done() -> Result<serde_json::Value, LocalhostError> {
    Ok(serde_json::Value::Null)
}

impl Relay {
    async fn handle(self: &Arc<Self>, request: MachineRequest) -> Result<serde_json::Value, LocalhostError> {
        let targets = &self.targets;
        match request {
            MachineRequest::Projects | MachineRequest::Subscribe => unreachable!("answered by forwarders"),
            MachineRequest::Ping => done(),
            MachineRequest::Start { service_id, on_port } => value(targets.start(service_id, on_port, ORIGIN).await?),
            MachineRequest::Stop { service_id } => value(targets.stop(service_id, ORIGIN).await?),
            MachineRequest::Restart { service_id } => value(targets.restart(service_id, ORIGIN).await?),
            MachineRequest::StartAll { project_id } => {
                targets.start_all(project_id, ORIGIN).await?;
                done()
            }
            MachineRequest::StopAll { project_id } => {
                targets.stop_all(project_id, ORIGIN).await?;
                done()
            }
            MachineRequest::OpenRunLog { stream, run_id } => {
                let (lines, receiver) = targets.subscribe_to_run(run_id).await?;
                let output = self.output.clone();
                self.open_stream(
                    stream,
                    forward_batches(receiver, move |lines, is_finished| {
                        output.send(MachineMessage::RunLog { stream, lines, is_finished }).is_ok()
                    }),
                );
                value(lines)
            }
            MachineRequest::OpenServicesLog { stream, service_ids } => {
                let (lines, receiver) = targets.subscribe_to_services(service_ids).await?;
                let output = self.output.clone();
                self.open_stream(
                    stream,
                    forward_batches(receiver, move |lines, _| {
                        output.send(MachineMessage::ServicesLog { stream, lines }).is_ok()
                    }),
                );
                value(lines)
            }
            MachineRequest::CloseLog { stream } => {
                if let Some(stream) = self.streams.lock().unwrap().remove(&stream) {
                    stream.abort();
                }
                done()
            }
            MachineRequest::Unsupported => Err(LocalhostError::Unsupported),
        }
    }

    /// Replies with the Projects, then sends them again after every change:
    /// an edit here, or one another client of the shared core announced.
    fn forward_projects(self: &Arc<Self>, id: u64) -> JoinHandle<()> {
        let (projects, mut events) = self.library.subscribe();
        let _ = self.output.send(MachineMessage::Reply { id, result: value(projects) });
        let relay = self.clone();
        tokio::spawn(async move {
            while events.recv().await.is_some() {
                while events.try_recv().is_ok() {}
                let projects = relay.library.projects();
                if relay.output.send(MachineMessage::Projects { projects }).is_err() {
                    return;
                }
            }
        })
    }

    /// Replies with every status, then sends every event after them.
    fn forward_events(self: &Arc<Self>, id: u64) -> JoinHandle<()> {
        let (statuses, mut events) = self.targets.subscribe();
        let _ = self.output.send(MachineMessage::Reply { id, result: value(statuses) });
        let output = self.output.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if output.send(MachineMessage::Event { event }).is_err() {
                    return;
                }
            }
        })
    }

    fn open_stream(self: &Arc<Self>, stream: u64, forward: impl Future<Output = ()> + Send + 'static) {
        let relay = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            forward.await;
            if let Some(relay) = relay.upgrade() {
                relay.streams.lock().unwrap().remove(&stream);
            }
        });
        if let Some(previous) = self.streams.lock().unwrap().insert(stream, task) {
            previous.abort();
        }
    }
}

async fn write_messages<W: AsyncWrite + Unpin>(mut receiver: mpsc::UnboundedReceiver<MachineMessage>, mut output: W) {
    while let Some(message) = receiver.recv().await {
        let mut line = serde_json::to_vec(&message).expect("a message always encodes");
        line.push(b'\n');
        if output.write_all(&line).await.is_err() || output.flush().await.is_err() {
            return;
        }
    }
}
