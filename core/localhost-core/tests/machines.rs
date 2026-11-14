//! Machines: the app of one Machine viewing and controlling another's
//! Projects through `machine::relay` (what `localhost remote --stdio`
//! serves over SSH), here over an in-memory pipe, with real processes.
#![cfg(unix)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Fixture, PATIENCE, free_port, key, test_options};
use localhost_core::machine::{MachineConnection, relay};
use localhost_core::{
    KeyClash, Link, LocalhostError, MachineProblem, NewService, Origin, ProfileInput, Project, ProjectChanges,
    ProjectDocument, ProjectLibrary, ServiceChanges, ServiceEvent, ServiceState, Targets, VariableInput, VariableLayer,
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

// MARK: Its Project Configuration

#[tokio::test(flavor = "multi_thread")]
async fn adds_a_project_there_from_a_folder_scanned_there() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    assert!(connection.manages_projects());
    let (_, mut changes) = connection.projects().await.unwrap();
    let home = other._fixture.path().join("shop");
    std::fs::create_dir_all(home.join("web")).unwrap();
    std::fs::write(home.join("package.json"), r#"{"scripts": {"dev": "vite"}}"#).unwrap();
    let path = home.to_str().unwrap().to_string();

    // The picker sees the Machine's folders.
    let listing = connection.list_directory(Some(path.clone()), false, false).await.unwrap();
    assert_eq!(listing.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["web"]);
    let listing = connection.list_directory(Some(path.clone()), false, true).await.unwrap();
    assert!(listing.entries.iter().any(|entry| entry.name == "package.json" && !entry.is_directory));

    let suggestions = connection.scan_folder(path.clone()).await.unwrap();
    let dev = suggestions.iter().find(|suggestion| suggestion.command_line.contains("dev")).expect("no dev script");
    let services = vec![NewService::from(dev.clone())];
    let project = connection.add_project("Shop".into(), key("shop"), path.clone(), services).await.unwrap();
    assert_eq!(other.library.project(project.id).unwrap().services.len(), 1);
    until(&mut changes, |projects| projects.iter().any(|shown| shown.id == project.id)).await;

    // Path errors are the Machine's.
    let missing = other._fixture.path().join("gone").to_str().unwrap().to_string();
    let error = connection.add_project("Gone".into(), key("gone"), missing.clone(), vec![]).await.unwrap_err();
    assert_eq!(error, LocalhostError::ProjectFolderMissing(missing.clone()));
    assert_eq!(connection.scan_folder(missing).await.unwrap_err(), error);
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_projects_and_services_there_keeping_secrets_there() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    let project_id = other.project.id;
    let changes = ProjectChanges {
        name: Some("App 2".into()),
        is_favorite: Some(true),
        variables: Some(vec![VariableInput::plain("MODE", "dev"), VariableInput::secret("TOKEN", "s3cret")]),
        ..Default::default()
    };
    let project = connection.edit_project(project_id, changes).await.unwrap();
    assert!((project.name.as_str(), project.is_favorite) == ("App 2", true));
    assert_eq!(project.variables.secret_names, ["TOKEN"]);
    // The value is in the Machine's secret store, and revealed on request.
    let layer = VariableLayer::Project(project_id);
    assert_eq!(other.library.secret_value("TOKEN", layer).unwrap().as_deref(), Some("s3cret"));
    assert_eq!(connection.secret_value("TOKEN".into(), layer).await.unwrap().as_deref(), Some("s3cret"));

    let port = free_port();
    let web = NewService { port: Some(port.into()), ..NewService::new("Web", key("web"), "exec sleep 60") };
    let web = connection.add_service(project_id, web, vec![], vec![]).await.unwrap();
    let worker = NewService::new("Worker", key("worker"), "exec sleep 60");
    let added = connection.add_services(project_id, vec![worker]).await.unwrap();
    // `Some(None)` removes the Port; leaving a field out keeps it.
    let changes = ServiceChanges {
        port: Some(None),
        dependencies: Some(vec![web.id]),
        run_before: Some(Some("true".into())),
        ..Default::default()
    };
    let worker = connection.edit_service(added[0].id, changes).await.unwrap();
    assert_eq!(
        (worker.port, worker.dependencies.clone(), worker.run_before.as_deref()),
        (None, vec![web.id], Some("true"))
    );
    let changes = ServiceChanges { name: Some("Web app".into()), ..Default::default() };
    assert_eq!(connection.edit_service(web.id, changes).await.unwrap().port, Some(port));

    let profiles = vec![ProfileInput {
        profile_id: None,
        name: "Testing".into(),
        variables: vec![VariableInput::plain("MODE", "test")],
    }];
    connection
        .save_environment(project_id, vec![VariableInput::existing_secret("TOKEN")], profiles, Some(0))
        .await
        .unwrap();
    let project = other.library.project(project_id).unwrap();
    assert_eq!(project.active_profile().map(|profile| profile.name.as_str()), Some("Testing"));
    assert_eq!(project.variables.secret_names, ["TOKEN"]);
    assert_eq!(other.library.secret_value("TOKEN", layer).unwrap().as_deref(), Some("s3cret"));

    // Removing a running Service stops it first, there.
    let (_, mut events) = connection.subscribe().await.unwrap();
    connection.start(web.id, None).await.unwrap();
    until(&mut events, reached(ServiceState::Starting)).await;
    connection.remove_service(web.id).await.unwrap();
    let project = other.library.project(project_id).unwrap();
    assert!(project.service(web.id).is_none());
    assert!(project.services[0].dependencies.is_empty());

    connection.remove_project(project_id).await.unwrap();
    assert!(other.library.projects().is_empty());
    assert_eq!(connection.remove_project(project_id).await.unwrap_err(), LocalhostError::ProjectNotFound(project_id));
}

#[tokio::test(flavor = "multi_thread")]
async fn imports_a_project_file_that_is_there() {
    let (other, connection) = connect(relay::SILENCE_TIMEOUT).await;
    let folder = other._fixture.path().join("api");
    std::fs::create_dir_all(&folder).unwrap();
    let file = other._fixture.path().join("api.yaml");
    let mut document = ProjectDocument::from(&other.library.project(other.project.id).unwrap());
    document.set_path(folder.to_str().unwrap());
    std::fs::write(&file, document.to_yaml()).unwrap();

    let read = connection.read_project_file(file.to_str().unwrap().into()).await.unwrap();
    assert!(read.folder_exists);
    let error = connection.import_project(read.text.clone(), folder.to_str().unwrap().into(), None).await.unwrap_err();
    assert_eq!(error, LocalhostError::DuplicateProjectKey(key("app")));
    let imported =
        connection.import_project(read.text, folder.to_str().unwrap().into(), Some(KeyClash::KeepBoth)).await.unwrap();
    assert_eq!((imported.key.as_str(), imported.path.as_str()), ("app-2", folder.to_str().unwrap()));
    assert_eq!(other.library.projects().len(), 2);

    let missing = other._fixture.path().join("none.yaml");
    assert!(matches!(
        connection.read_project_file(missing.to_str().unwrap().into()).await,
        Err(LocalhostError::PersistenceFailed { .. })
    ));
}

/// A Machine with a localhost from before Projects could be changed from
/// another Machine: its Projects are shown and run, and every change says
/// to update localhost there, without a request it can't answer.
#[tokio::test(flavor = "multi_thread")]
async fn says_to_update_an_older_machine_to_manage_its_projects() {
    let (app, older) = tokio::io::duplex(1 << 16);
    let (older_reader, mut older_writer) = tokio::io::split(older);
    tokio::spawn(async move {
        older_writer
            .write_all(b"{\"kind\":\"hello\",\"machineProtocol\":1,\"version\":\"0.5.6\",\"hostName\":\"devbox\"}\n")
            .await
            .unwrap();
        // It answers what it knows and calls the rest unsupported, as 0.5.6 does.
        let mut lines = BufReader::new(older_reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let call: serde_json::Value = serde_json::from_str(&line).unwrap();
            let result = match call["request"].as_str() {
                Some("projects") => serde_json::json!({ "Ok": [] }),
                _ => serde_json::json!({ "Err": { "kind": "unsupported" } }),
            };
            let reply = serde_json::json!({ "kind": "reply", "id": call["id"], "result": result });
            older_writer.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
        }
    });
    let (reader, writer) = tokio::io::split(app);
    let connection = MachineConnection::connect("devbox", reader, writer).await.unwrap();
    assert!(!connection.manages_projects());
    assert!(connection.projects().await.unwrap().0.is_empty());

    let needs_update = LocalhostError::MachineNeedsUpdate { machine: "devbox".into() };
    assert_eq!(
        connection.add_project("Shop".into(), key("shop"), "/srv/shop".into(), vec![]).await.unwrap_err(),
        needs_update
    );
    assert_eq!(connection.list_directory(None, false, false).await.unwrap_err(), needs_update);
    assert_eq!(needs_update.message(), "Update localhost on devbox to manage its Projects.");
}
