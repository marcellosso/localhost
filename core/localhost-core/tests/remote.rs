//! The protocol between the app and `localhostd` (`localhost_core::remote`),
//! end to end over an in-memory pipe, with real processes.
#![cfg(unix)]

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::{Fixture, PATIENCE, key, process_group_exists, test_options};
use localhost_core::remote::{self, Hello, PROTOCOL_VERSION, Remote};
use localhost_core::{
    LocalhostError, NewService, Origin, Project, ProjectChanges, ProjectLibrary, ServiceChanges, ServiceEvent,
    ServiceState, VariableInput,
};
use tokio::io::{AsyncWriteExt, DuplexStream};
use tokio::task::JoinHandle;

/// The app's library on disk, and a `localhostd` serving over a pipe.
struct Setup {
    fixture: Fixture,
    library: ProjectLibrary,
    project: Project,
    server: JoinHandle<()>,
    remote: Remote,
}

async fn setup() -> Setup {
    let fixture = Fixture::new();
    let library = fixture.library();
    let folder = fixture.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project =
        library.add_project("Shop", key("shop"), folder.to_str().unwrap(), vec![], Origin::MainWindow).unwrap();

    let (client, server) = tokio::io::duplex(64 * 1024);
    let root = fixture.path().join("localhostd");
    let server = tokio::spawn(async move {
        let (input, output) = tokio::io::split(server);
        remote::serve(input, output, &root, test_options()).await;
    });
    let (reader, writer) = tokio::io::split(client);
    let remote = Remote::connect(reader, writer).await.unwrap();
    Setup { fixture, library, project, server, remote }
}

impl Setup {
    /// Sends the library's Projects, with the Secret values their Services
    /// read, as the app does before starting them all.
    async fn sync(&self) {
        let projects = self.library.projects();
        let services: Vec<_> =
            projects.iter().flat_map(|project| project.services.iter().map(|service| service.id)).collect();
        let secrets = self.library.secrets_for_runs(&services).unwrap();
        self.remote.set_projects(projects, secrets).await.unwrap();
    }
}

async fn until<T>(receiver: &mut tokio::sync::mpsc::UnboundedReceiver<T>, condition: impl Fn(&T) -> bool) -> T {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let item = receiver.recv().await.expect("the stream ended");
            if condition(&item) {
                return item;
            }
        }
    })
    .await
    .expect("waited too long")
}

fn reached(state: ServiceState) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::ServiceStateChanged(status) if status.state == state)
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_a_service_with_its_secret_and_streams_its_output() {
    let setup = setup().await;
    let service = setup
        .library
        .add_service(
            setup.project.id,
            NewService::new("Web", key("web"), "echo \"token=$TOKEN\"; sleep 30"),
            vec![VariableInput::secret("TOKEN", "s3cret")],
            vec![],
            Origin::MainWindow,
        )
        .unwrap();
    setup.sync().await;
    assert_eq!(setup.remote.hello(), &Hello::current());

    let (_, mut events) = setup.remote.subscribe();
    let (_, mut lines) = setup.remote.subscribe_to_services(HashSet::from([service.id])).await.unwrap();
    setup.remote.start(service.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(ServiceState::Running)).await;
    let line = until(&mut lines, |line| line.line.text.starts_with("token=")).await;
    assert_eq!(line.line.text, "token=s3cret");
    assert_eq!(setup.remote.status(service.id).state, ServiceState::Running);

    let run_id = setup.remote.status(service.id).run.unwrap().id;
    let (buffered, _) = setup.remote.subscribe_to_run(run_id).await.unwrap();
    assert!(buffered.iter().any(|line| line.text == "token=s3cret"));

    let stopped = setup.remote.stop(service.id, Origin::MainWindow).await.unwrap();
    assert_eq!(stopped.state, ServiceState::Stopped);
    assert_eq!(stopped.run.unwrap().stop_origin, Some(Origin::MainWindow));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_configuration_marks_the_run_as_needing_a_restart() {
    let setup = setup().await;
    let service = setup
        .library
        .add_service(
            setup.project.id,
            NewService::new("Web", key("web"), "sleep 30"),
            vec![],
            vec![],
            Origin::MainWindow,
        )
        .unwrap();
    setup.sync().await;
    let (_, mut events) = setup.remote.subscribe();
    setup.remote.start(service.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(ServiceState::Running)).await;

    let changes = ServiceChanges { command_line: Some("sleep 31".into()), ..Default::default() };
    setup.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    setup.sync().await;
    until(&mut events, |event| matches!(event, ServiceEvent::ServiceStateChanged(status) if status.is_restart_needed))
        .await;
    setup.remote.stop_every_run(Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_the_connection_stops_every_run() {
    let setup = setup().await;
    let pid_file = setup.fixture.path().join("pid");
    let command = format!("echo $$ > '{}'; sleep 30", pid_file.display());
    let service = setup
        .library
        .add_service(setup.project.id, NewService::new("Web", key("web"), command), vec![], vec![], Origin::MainWindow)
        .unwrap();
    setup.sync().await;
    let (_, mut events) = setup.remote.subscribe();
    setup.remote.start(service.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(ServiceState::Running)).await;
    let group: i32 = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    assert!(process_group_exists(group));

    // The app quits or crashes: its end of the pipe closes.
    drop(events);
    drop(setup.remote);
    tokio::time::timeout(PATIENCE, setup.server).await.expect("localhostd ended").unwrap();
    assert!(!process_group_exists(group));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_connection_fails_calls_and_ends_streams() {
    let setup = setup().await;
    setup.sync().await;
    let (_, mut events) = setup.remote.subscribe();
    setup.server.abort();

    tokio::time::timeout(PATIENCE, setup.remote.closed()).await.expect("noticed the loss");
    assert!(!setup.remote.is_connected());
    assert!(events.recv().await.is_none());
    assert!(matches!(
        setup.remote.start_all(setup.project.id, Origin::MainWindow).await,
        Err(LocalhostError::TargetUnavailable(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_errors_in_the_cores_words_and_scans_folders_there() {
    let setup = setup().await;
    setup.sync().await;
    let unknown = localhost_core::Id::new();
    assert_eq!(
        setup.remote.start(unknown, None, Origin::MainWindow).await,
        Err(LocalhostError::ServiceNotFound(unknown))
    );

    std::fs::write(std::path::Path::new(&setup.project.path).join("package.json"), r#"{"scripts": {"dev": "vite"}}"#)
        .unwrap();
    let suggestions = setup.remote.scan_folder(&setup.project.path).await.unwrap();
    assert!(suggestions.iter().any(|suggestion| suggestion.command_line.contains("dev")), "{suggestions:?}");
    assert!(setup.remote.orphans().await.unwrap().is_empty());
    assert!(setup.remote.listeners().await.is_ok());

    let changes = ProjectChanges { name: Some("Renamed".into()), ..Default::default() };
    setup.library.edit_project(setup.project.id, changes, Origin::MainWindow).unwrap();
    setup.sync().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_localhostd_that_speaks_another_protocol() {
    let (client, mut server): (DuplexStream, DuplexStream) = tokio::io::duplex(1024);
    let hello = format!("{{\"kind\":\"hello\",\"protocol\":{},\"version\":\"9.9.9\"}}\n", PROTOCOL_VERSION + 1);
    server.write_all(hello.as_bytes()).await.unwrap();
    let (reader, writer) = tokio::io::split(client);
    let error = Remote::connect(reader, writer).await.err().unwrap();
    assert!(matches!(&error, LocalhostError::TargetUnavailable(reason) if reason.contains("9.9.9")), "{error:?}");
    let _ = Arc::new(server);
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_secret_values_it_was_sent_until_no_project_names_them() {
    let setup = setup().await;
    let add = |variables| {
        let service = NewService::new("Web", key("web"), "echo \"token=$TOKEN\"; sleep 30");
        setup.library.add_service(setup.project.id, service, variables, vec![], Origin::MainWindow).unwrap()
    };
    let service = add(vec![VariableInput::secret("TOKEN", "s3cret")]);
    setup.sync().await;
    // Another client's Projects, with no Secret value: an agent starting something else.
    setup.remote.set_projects(setup.library.projects(), HashMap::new()).await.unwrap();

    let (_, mut lines) = setup.remote.subscribe_to_services(HashSet::from([service.id])).await.unwrap();
    setup.remote.start(service.id, None, Origin::Agent).await.unwrap();
    let line = until(&mut lines, |line| line.line.text.starts_with("token=")).await;
    assert_eq!(line.line.text, "token=s3cret");
    setup.remote.stop(service.id, Origin::Agent).await.unwrap();

    // Once no Project names it, the value is gone, even if the name comes back.
    let edit = |variables| {
        let changes = ServiceChanges { variables: Some(variables), ..Default::default() };
        setup.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    };
    edit(vec![]);
    setup.remote.set_projects(setup.library.projects(), HashMap::new()).await.unwrap();
    edit(vec![VariableInput::secret("TOKEN", "s3cret")]);
    setup.remote.set_projects(setup.library.projects(), HashMap::new()).await.unwrap();
    let failed = setup.remote.start(service.id, None, Origin::Agent).await.unwrap();
    assert_eq!(failed.run.unwrap().failure, Some(LocalhostError::SecretMissing("TOKEN".into())));
}
