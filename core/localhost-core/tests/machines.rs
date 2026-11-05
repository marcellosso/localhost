//! Machines: the app of one Machine viewing and controlling another's
//! Projects through `machine::relay` (what `localhost remote --stdio`
//! serves over SSH), here over an in-memory pipe, with real processes.
#![cfg(unix)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Fixture, PATIENCE, key, test_options};
use localhost_core::machine::{MachineConnection, relay};
use localhost_core::{
    Link, LocalhostError, MachineProblem, NewService, Origin, Project, ProjectLibrary, ServiceEvent, ServiceState,
    Targets,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

/// The other Machine: its library and shared core (here in this process),
/// and the relay serving them to one app.
struct Other {
    _fixture: Fixture,
    library: Arc<ProjectLibrary>,
    project: Project,
    relay: JoinHandle<()>,
}

fn other_machine() -> (Fixture, Arc<ProjectLibrary>, Targets, Project) {
    let fixture = Fixture::new();
    let library = Arc::new(fixture.library());
    let folder = fixture.path().join("app");
    std::fs::create_dir_all(&folder).unwrap();
    let project = library.add_project("App", key("app"), folder.to_str().unwrap(), vec![], Origin::Agent).unwrap();
    let targets = Targets::open(library.clone(), fixture.path(), test_options(), Box::new(|_| Link::InProcess));
    (fixture, library, targets, project)
}

async fn connect(silence: Duration) -> (Other, MachineConnection) {
    let (fixture, library, targets, project) = other_machine();
    let (app_end, relay_end) = tokio::io::duplex(256 * 1024);
    let (input, output) = tokio::io::split(relay_end);
    let relay = tokio::spawn(relay::serve(input, output, library.clone(), targets, silence));
    let (reader, writer) = tokio::io::split(app_end);
    let connection = MachineConnection::connect("devbox", reader, writer).await.unwrap();
    (Other { _fixture: fixture, library, project, relay }, connection)
}

fn add(other: &Other, name: &str, command: &str) -> localhost_core::Service {
    let service = NewService::new(name, key(name), command);
    other.library.add_service(other.project.id, service, vec![], vec![], Origin::Agent).unwrap()
}

async fn until<T>(receiver: &mut UnboundedReceiver<T>, condition: impl Fn(&T) -> bool) -> T {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let item = receiver.recv().await.expect("the stream ended");
            if condition(&item) {
                return item;
            }
        }
    })
    .await
    .expect("timed out")
}

fn reached(state: ServiceState) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::ServiceStateChanged(status) if status.state == state)
}

#[tokio::test(flavor = "multi_thread")]
async fn shows_the_other_machines_projects_as_they_change() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    assert_eq!(connection.hello().machine_protocol, localhost_core::machine::MACHINE_PROTOCOL_VERSION);

    let (projects, mut changes) = connection.projects().await.unwrap();
    assert_eq!(projects.iter().map(|project| project.key.as_str()).collect::<Vec<_>>(), ["app"]);

    // An agent there adds a Service.
    add(&other, "web", "exec sleep 60");
    let projects = until(&mut changes, |projects| projects[0].services.len() == 1).await;
    assert_eq!(projects[0].services[0].key.as_str(), "web");
}

#[tokio::test(flavor = "multi_thread")]
async fn controls_services_there_as_another_machine() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    let web = add(&other, "web", "echo hello from devbox; exec sleep 60");
    let (statuses, mut events) = connection.subscribe().await.unwrap();
    assert!(statuses.values().all(|status| !status.state.is_active()));

    let status = connection.start(web.id, None).await.unwrap();
    assert_eq!(status.run.as_ref().unwrap().origin, Origin::Machine);
    until(&mut events, reached(ServiceState::Running)).await;

    let run_id = status.run.unwrap().id;
    let (lines, mut more) = connection.subscribe_to_run(run_id).await.unwrap();
    if !lines.iter().any(|line| line.text == "hello from devbox") {
        until(&mut more, |line| line.text == "hello from devbox").await;
    }

    let status = connection.stop(web.id).await.unwrap();
    assert_eq!(status.state, ServiceState::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_fail_once_the_connection_is_lost() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    let web = add(&other, "web", "exec sleep 60");
    other.relay.abort();
    drop(other.relay);
    tokio::time::timeout(PATIENCE, connection.closed()).await.expect("the loss went unnoticed");

    let error = connection.start(web.id, None).await.unwrap_err();
    assert_eq!(error, LocalhostError::MachineUnavailable { machine: "devbox".into(), problem: MachineProblem::Lost });
    assert!(!connection.is_connected());
}

/// The raw protocol, as an app of another version speaks it.
async fn raw(silence: Duration) -> (Other, impl AsyncBufReadExt + Unpin, impl AsyncWriteExt + Unpin) {
    let (fixture, library, targets, project) = other_machine();
    let (app_end, relay_end) = tokio::io::duplex(256 * 1024);
    let (input, output) = tokio::io::split(relay_end);
    let relay = tokio::spawn(relay::serve(input, output, library.clone(), targets, silence));
    let (reader, writer) = tokio::io::split(app_end);
    (Other { _fixture: fixture, library, project, relay }, BufReader::new(reader), writer)
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_a_newer_apps_request_as_unsupported() {
    let (_other, mut reader, mut writer) = raw(relay::SILENCE_TIMEOUT).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.contains("\"kind\":\"hello\""), "{line}");

    writer.write_all(b"{\"id\":7,\"request\":\"teleport\",\"to\":\"mars\"}\n").await.unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["result"]["Err"]["kind"], "unsupported");
}

#[tokio::test(flavor = "multi_thread")]
async fn lets_go_of_an_app_that_went_silent() {
    // A laptop that slept never closes its SSH connection.
    let (other, mut reader, _writer) = raw(Duration::from_millis(300)).await;
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    tokio::time::timeout(PATIENCE, other.relay).await.expect("still waiting for a silent app").unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn ends_when_the_app_closes_the_connection() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    connection.close();
    tokio::time::timeout(PATIENCE, other.relay).await.expect("the relay didn't end").unwrap();
}

/// localhost there starts but can't reach its own shared core: the app says
/// so in that Machine's words, not as a failure of SSH.
#[tokio::test]
async fn says_why_the_other_machine_cant_serve() {
    let (app, relay_end) = tokio::io::duplex(1 << 16);
    let (relay_reader, relay_writer) = tokio::io::split(relay_end);
    drop(relay_reader);
    let reason = LocalhostError::IncompatibleCore { version: "0.4.0".into() }.message();
    relay::refuse(relay_writer, &reason).await;
    let (reader, writer) = tokio::io::split(app);
    let result = MachineConnection::connect("devbox", reader, writer).await;
    let Err(error) = result else { panic!("it connected") };
    assert_eq!(
        error,
        LocalhostError::MachineUnavailable { machine: "devbox".into(), problem: MachineProblem::CantServe(reason) }
    );
    assert!(error.message().starts_with("localhost on devbox couldn’t start: localhost 0.4.0 is still running"));
}
