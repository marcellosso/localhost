//! The shared core (`localhost_core::remote::shared`): several clients on
//! one socket driving one Service manager, with real processes, and what
//! each one disconnecting stops.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{Fixture, PATIENCE, key, test_options, wait_until};
use localhost_core::remote::{Client, Remote, shared};
use localhost_core::{Id, NewService, Origin, Project, ProjectLibrary, Service, ServiceState};
use tokio::task::JoinHandle;

struct Setup {
    fixture: Fixture,
    library: ProjectLibrary,
    project: Project,
    core: JoinHandle<()>,
}

/// A core serving the fixture's storage root, gone after `idle_timeout`
/// without clients.
async fn setup(idle_timeout: Duration) -> Setup {
    let fixture = Fixture::new();
    let library = fixture.library();
    let folder = fixture.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project =
        library.add_project("Shop", key("shop"), folder.to_str().unwrap(), vec![], Origin::MainWindow).unwrap();
    let root = fixture.path().to_path_buf();
    let listener = shared::bind(&root).unwrap();
    let core = tokio::spawn(async move {
        shared::serve(listener, &root, test_options(), idle_timeout, std::future::pending()).await;
    });
    Setup { fixture, library, project, core }
}

impl Setup {
    fn service(&self, key_text: &str, dependencies: Vec<Id>) -> Service {
        let service = NewService::new(key_text.to_uppercase(), key(key_text), "sleep 1000");
        self.library.add_service(self.project.id, service, vec![], dependencies, Origin::MainWindow).unwrap()
    }

    /// A client of the core, with the Projects sent, as `Targets` sends them.
    async fn connect(&self, client: Client) -> Remote {
        let remote = shared::connect(self.fixture.path(), client, None).await.unwrap();
        remote.set_projects(self.library.projects(), Default::default()).await.unwrap();
        remote
    }
}

async fn wait_for(remote: &Remote, service_id: Id, state: ServiceState) {
    wait_until(|| remote.status(service_id).state == state).await;
}

/// Gives a disconnect time to stop what it would wrongly stop.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_one_client_starts_is_the_run_every_client_sees_and_stops() {
    let setup = setup(PATIENCE).await;
    let web = setup.service("web", vec![]);
    let app = setup.connect(Client::App).await;
    let agent = setup.connect(Client::Agent).await;

    agent.start(web.id, None, Origin::Agent).await.unwrap();
    wait_for(&app, web.id, ServiceState::Running).await;
    let run = app.status(web.id).run.unwrap();
    assert_eq!(run.origin, Origin::Agent);
    assert_eq!(agent.status(web.id).run.unwrap().id, run.id);

    app.stop(web.id, Origin::MainWindow).await.unwrap();
    wait_for(&agent, web.id, ServiceState::Stopped).await;
    assert_eq!(agent.status(web.id).run.unwrap().stop_origin, Some(Origin::MainWindow));
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_stops_while_an_app_is_connected_then_what_nobody_holds_does() {
    let setup = setup(PATIENCE).await;
    let web = setup.service("web", vec![]);
    let app = setup.connect(Client::App).await;
    let agent = setup.connect(Client::Agent).await;
    let watcher = setup.connect(Client::Cli).await;

    agent.start(web.id, None, Origin::Agent).await.unwrap();
    wait_for(&app, web.id, ServiceState::Running).await;
    drop(agent);
    settle().await;
    assert_eq!(app.status(web.id).state, ServiceState::Running);

    // An app gone (quit without stopping, or crashed): no one holds it now.
    drop(app);
    wait_for(&watcher, web.id, ServiceState::Stopped).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_keeps_what_it_started_and_its_dependencies() {
    let setup = setup(PATIENCE).await;
    let db = setup.service("db", vec![]);
    let web = setup.service("web", vec![db.id]);
    let worker = setup.service("worker", vec![]);
    let app = setup.connect(Client::App).await;
    let agent = setup.connect(Client::Agent).await;

    app.start(db.id, None, Origin::MainWindow).await.unwrap();
    app.start(worker.id, None, Origin::MainWindow).await.unwrap();
    agent.start(web.id, None, Origin::Agent).await.unwrap();
    wait_for(&agent, web.id, ServiceState::Running).await;
    wait_for(&agent, worker.id, ServiceState::Running).await;

    drop(app);
    // The app's worker stops; db stays, as the agent's web needs it.
    wait_for(&agent, worker.id, ServiceState::Stopped).await;
    assert_eq!(agent.status(db.id).state, ServiceState::Running);
    assert_eq!(agent.status(web.id).state, ServiceState::Running);

    let watcher = setup.connect(Client::Cli).await;
    drop(agent);
    wait_for(&watcher, web.id, ServiceState::Stopped).await;
    wait_for(&watcher, db.id, ServiceState::Stopped).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_run_is_held_by_whoever_starts_it_next() {
    let setup = setup(PATIENCE).await;
    let web = setup.service("web", vec![]);
    let first = setup.connect(Client::Agent).await;
    let second = setup.connect(Client::Agent).await;

    first.start(web.id, None, Origin::Agent).await.unwrap();
    wait_for(&second, web.id, ServiceState::Running).await;
    second.stop(web.id, Origin::Agent).await.unwrap();
    second.start(web.id, None, Origin::Agent).await.unwrap();
    wait_for(&second, web.id, ServiceState::Running).await;

    drop(first);
    settle().await;
    assert_eq!(second.status(web.id).state, ServiceState::Running);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_core_exits_once_no_client_has_been_connected_for_a_while() {
    let setup = setup(Duration::from_millis(300)).await;
    let web = setup.service("web", vec![]);
    let app = setup.connect(Client::App).await;
    app.start(web.id, None, Origin::MainWindow).await.unwrap();
    wait_for(&app, web.id, ServiceState::Running).await;

    // Still serving while connected, however long.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(!setup.core.is_finished());
    drop(app);
    tokio::time::timeout(PATIENCE, setup.core).await.expect("the core exits").unwrap();
    let error = shared::connect(setup.fixture.path(), Client::Agent, None).await.err().unwrap();
    assert!(matches!(error, localhost_core::LocalhostError::CoreUnavailable(_)), "{error:?}");
}

#[test]
fn one_core_per_storage_root_and_only_its_user_may_connect() {
    let fixture = Fixture::new();
    let lock = shared::lock(fixture.path()).unwrap();
    assert!(lock.is_some());
    assert!(shared::lock(fixture.path()).unwrap().is_none(), "a second core finds it taken");
    drop(lock);
    assert!(shared::lock(fixture.path()).unwrap().is_some(), "free again once the first is gone");

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _listener = runtime.block_on(async { shared::bind(fixture.path()).unwrap() });
    let mode = std::fs::metadata(shared::socket_path(fixture.path())).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_socket_path_too_long_for_the_system_moves_to_the_temporary_folder() {
    let short = std::path::Path::new("/tmp/localhost");
    assert_eq!(shared::socket_path(short), short.join("localhostd.sock"));
    let long = std::path::PathBuf::from(format!("/tmp/{}", "a".repeat(120)));
    let moved = shared::socket_path(&long);
    assert!(moved.starts_with(std::env::temp_dir()), "{moved:?}");
    assert_eq!(moved, shared::socket_path(&long), "the same for every client");
    assert_ne!(moved, shared::socket_path(&long.join("other")));
}
