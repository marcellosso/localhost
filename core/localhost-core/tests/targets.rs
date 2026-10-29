//! `Targets`: Projects on this machine run in this process, and Projects in a
//! WSL distro through a `localhostd` (here `remote::serve` over an in-memory
//! pipe, on this machine), with real processes.
#![cfg(unix)]

mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::{Fixture, PATIENCE, key, process_group_exists, test_options};
use localhost_core::remote::{self, Remote};
use localhost_core::{
    ExecutionTarget, Link, LocalhostError, MemorySecretStore, NewService, Origin, Project, ProjectLibrary, RunRegistry,
    SecretStore, ServiceChanges, ServiceEvent, ServiceManager, ServiceState, Targets, VariableInput, VariableLayer,
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

fn distro(name: &str) -> ExecutionTarget {
    ExecutionTarget::Wsl { distro: name.into() }
}

/// The app's library, its Targets, and the `localhostd`s they started.
struct Setup {
    fixture: Fixture,
    library: Arc<ProjectLibrary>,
    targets: Targets,
    here: Project,
    there: Project,
    /// How many times the distro's `localhostd` was started.
    launches: Arc<AtomicUsize>,
    /// Each connection's pipe, to cut it as a crash of `localhostd` would.
    cables: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// The accounts the app's library read from its secret store.
    reads: Arc<Mutex<Vec<String>>>,
}

/// The app's secret store, noting each read, as a Keychain would prompt.
struct Recording {
    store: Arc<MemorySecretStore>,
    reads: Arc<Mutex<Vec<String>>>,
}

impl SecretStore for Recording {
    fn value(&self, account: &str) -> Result<Option<String>, LocalhostError> {
        self.reads.lock().unwrap().push(account.to_string());
        self.store.value(account)
    }

    fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError> {
        self.store.set_value(value, account)
    }

    fn remove_value(&self, account: &str) -> Result<(), LocalhostError> {
        self.store.remove_value(account)
    }
}

/// The storage root of the distro's `localhostd`.
fn distro_root(fixture: &Fixture) -> PathBuf {
    fixture.path().join("localhostd")
}

fn setup() -> Setup {
    let fixture = Fixture::new();
    let reads = Arc::new(Mutex::new(Vec::new()));
    let store = Recording { store: fixture.secrets.clone(), reads: reads.clone() };
    let library = Arc::new(ProjectLibrary::open(fixture.path(), Arc::new(store)).unwrap());
    let folder = |name: &str| {
        let folder = fixture.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder.to_str().unwrap().to_string()
    };
    let here = library.add_project("Here", key("here"), &folder("here"), vec![], Origin::MainWindow).unwrap();
    let there = library
        .add_project_on(distro("Test"), "There", key("there"), &folder("there"), vec![], Origin::MainWindow)
        .unwrap();

    let launches = Arc::new(AtomicUsize::new(0));
    let cables = Arc::new(Mutex::new(Vec::new()));
    let (root, counted, laid) = (distro_root(&fixture), launches.clone(), cables.clone());
    let launcher = Box::new(move |target: &ExecutionTarget| match target {
        ExecutionTarget::ThisMachine => Link::InProcess,
        ExecutionTarget::Wsl { distro } if distro == "Test" => {
            counted.fetch_add(1, Ordering::SeqCst);
            let (client, mut app_end) = tokio::io::duplex(64 * 1024);
            let (mut localhostd_end, server) = tokio::io::duplex(64 * 1024);
            let root = root.clone();
            tokio::spawn(async move {
                let (input, output) = tokio::io::split(server);
                remote::serve(input, output, &root, test_options()).await;
            });
            laid.lock().unwrap().push(tokio::spawn(async move {
                let _ = tokio::io::copy_bidirectional(&mut app_end, &mut localhostd_end).await;
            }));
            let (reader, writer) = tokio::io::split(client);
            Link::Remote(Box::pin(Remote::connect(reader, writer)))
        }
        ExecutionTarget::Wsl { distro } => Link::Unavailable(format!("{distro} isn't installed")),
    });
    let targets = Targets::open(library.clone(), fixture.path(), test_options(), launcher);
    Setup { fixture, library, targets, here, there, launches, cables, reads }
}

impl Setup {
    fn add(
        &self,
        project: &Project,
        name: &str,
        command: &str,
        variables: Vec<VariableInput>,
    ) -> localhost_core::Service {
        let service = NewService::new(name, key(&name.to_lowercase()), command);
        self.library.add_service(project.id, service, variables, vec![], Origin::MainWindow).unwrap()
    }

    /// The accounts read since the last call.
    fn take_reads(&self) -> HashSet<String> {
        self.reads.lock().unwrap().drain(..).collect()
    }
}

fn account(service: &localhost_core::Service, name: &str) -> String {
    VariableLayer::Service(service.id).secret_account(name)
}

async fn output_of(targets: &Targets, service_id: localhost_core::Id, text: &str) {
    let run_id = targets.status(service_id).run.unwrap().id;
    let (lines, mut more) = targets.subscribe_to_run(run_id).await.unwrap();
    if !lines.iter().any(|line| line.text == text) {
        until(&mut more, |line| line.text == text).await;
    }
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
    .expect("waited too long")
}

fn reached(service_id: localhost_core::Id, state: ServiceState) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::ServiceStateChanged(status) if status.service_id == service_id && status.state == state)
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_each_project_where_it_belongs_and_merges_their_statuses() {
    let setup = setup();
    let local = setup.add(&setup.here, "Local", "echo here; sleep 30", vec![]);
    let distant = setup.add(
        &setup.there,
        "Distant",
        "echo \"there $TOKEN\"; sleep 30",
        vec![VariableInput::secret("TOKEN", "s3cret")],
    );
    let (_, mut events) = setup.targets.subscribe();

    setup.targets.start(local.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(local.id, ServiceState::Running)).await;
    assert_eq!(setup.launches.load(Ordering::SeqCst), 0, "the distro is connected only once it is needed");

    setup.targets.start(distant.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(distant.id, ServiceState::Running)).await;
    assert_eq!(setup.launches.load(Ordering::SeqCst), 1);
    let (statuses, _) = setup.targets.subscribe();
    assert_eq!(statuses[&local.id].state, ServiceState::Running);
    assert_eq!(statuses[&distant.id].state, ServiceState::Running);

    // Both Services' logs, in one stream, from where each one runs.
    let (buffered, _) = setup.targets.subscribe_to_services(HashSet::from([local.id, distant.id])).await.unwrap();
    let texts: HashSet<&str> = buffered.iter().map(|line| line.line.text.as_str()).collect();
    assert!(texts.contains("here") && texts.contains("there s3cret"), "{texts:?}");
    let run_id = setup.targets.status(distant.id).run.unwrap().id;
    let (lines, _) = setup.targets.subscribe_to_run(run_id).await.unwrap();
    assert!(lines.iter().any(|line| line.text == "there s3cret"));
    setup.targets.clear_buffer(distant.id).await.unwrap();
    assert!(setup.targets.subscribe_to_run(run_id).await.unwrap().0.is_empty(), "cleared in the distro");
    // The distro's Runs keep their logs and records there.
    assert!(distro_root(&setup.fixture).join("runs").is_dir());

    let listeners = setup.targets.listeners().await;
    assert!(listeners.iter().all(|(target, _)| *target == ExecutionTarget::ThisMachine || *target == distro("Test")));

    // Stats are measured where each Run is, the previous sample sent along.
    for service_id in [local.id, distant.id] {
        let first = setup.targets.run_stats(service_id, None).await.unwrap().expect("the Run is active");
        assert_eq!(first.run_id, setup.targets.status(service_id).run.unwrap().id);
        assert!(first.process_count >= 1 && first.memory_bytes > 0, "{first:?}");
        let second = setup.targets.run_stats(service_id, Some(first)).await.unwrap().unwrap();
        assert!(second.cpu_percent.is_some(), "{second:?}");
    }

    setup.targets.stop_every_run(Origin::MainWindow).await;
    assert_eq!(setup.targets.status(local.id).state, ServiceState::Stopped);
    assert_eq!(setup.targets.status(distant.id).state, ServiceState::Stopped);
    assert_eq!(setup.targets.run_stats(distant.id, None).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_right_after_an_edit_runs_what_was_saved() {
    let setup = setup();
    let service = setup.add(&setup.there, "Web", "echo v1; sleep 30", vec![]);
    let (_, mut events) = setup.targets.subscribe();
    setup.targets.start(service.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(service.id, ServiceState::Running)).await;

    // The change reaches the distro on its own: the Run needs a restart.
    let changes = ServiceChanges { command_line: Some("echo v2; sleep 30".into()), ..Default::default() };
    setup.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    until(&mut events, |event| matches!(event, ServiceEvent::ServiceStateChanged(status) if status.is_restart_needed))
        .await;
    setup.targets.restart(service.id, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(service.id, ServiceState::Running)).await;

    // Edited and started at once: the start waits for the Projects to arrive.
    let changes = ServiceChanges { command_line: Some("echo v3; sleep 30".into()), ..Default::default() };
    setup.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    // The restart's own status: the merged one may still hold the Run it ended
    // until the distro's events arrive.
    let restarted = setup.targets.restart(service.id, Origin::MainWindow).await.unwrap();
    let run_id = restarted.run.unwrap().id;
    let (lines, mut more) = setup.targets.subscribe_to_run(run_id).await.unwrap();
    if !lines.iter().any(|line| line.text == "v3") {
        until(&mut more, |line| line.text == "v3").await;
    }
    setup.targets.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_distro_fails_its_runs_and_is_started_again_when_needed() {
    let setup = setup();
    let local = setup.add(&setup.here, "Local", "sleep 30", vec![]);
    let distant = setup.add(&setup.there, "Distant", "sleep 30", vec![]);
    let (_, mut events) = setup.targets.subscribe();
    setup.targets.start(local.id, None, Origin::MainWindow).await.unwrap();
    setup.targets.start(distant.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(distant.id, ServiceState::Running)).await;

    // localhostd dies (wsl --shutdown, a crash).
    setup.cables.lock().unwrap()[0].abort();
    let failed = until(&mut events, reached(distant.id, ServiceState::Failed)).await;
    let ServiceEvent::ServiceStateChanged(failed) = failed else { unreachable!() };
    assert!(matches!(failed.run.unwrap().failure, Some(LocalhostError::TargetUnavailable(_))));
    assert_ne!(setup.targets.status(local.id).state, ServiceState::Failed, "this machine's Runs are untouched");

    setup.targets.start(distant.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(distant.id, ServiceState::Running)).await;
    assert_eq!(setup.launches.load(Ordering::SeqCst), 2);
    setup.targets.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_target_that_cannot_be_reached_says_why() {
    let setup = setup();
    let changes = localhost_core::ProjectChanges { execution_target: Some(distro("Gone")), ..Default::default() };
    setup.library.edit_project(setup.there.id, changes, Origin::MainWindow).unwrap();
    let service = setup.add(&setup.there, "Web", "sleep 30", vec![]);
    assert_eq!(
        setup.targets.start(service.id, None, Origin::MainWindow).await,
        Err(LocalhostError::TargetUnavailable("Gone isn't installed".into()))
    );
    assert_eq!(setup.targets.status(service.id).state, ServiceState::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_distro_reports_orphans_of_an_earlier_session_as_it_connects() {
    let setup = setup();
    let service = setup.add(&setup.there, "Web", "sleep 30", vec![]);
    // An earlier localhostd that ended without stopping its Run.
    let earlier_library = Arc::new(ProjectLibrary::mirror(Arc::new(localhost_core::MemorySecretStore::new())));
    earlier_library.replace_projects(setup.library.projects(), Origin::MainWindow);
    let root = distro_root(&setup.fixture);
    let earlier = ServiceManager::new(
        earlier_library.clone(),
        Arc::new(localhost_core::LogStore::new(&root)),
        Arc::new(RunRegistry::new(&root)),
        test_options(),
    );
    earlier.start(service.id, None, Origin::MainWindow).await.unwrap();

    let mut found = setup.targets.found_orphans();
    setup.targets.scan_project(&setup.there).await.unwrap();
    let found = tokio::time::timeout(PATIENCE, found.recv()).await.unwrap().unwrap();
    assert_eq!(found.target, distro("Test"));
    assert_eq!(found.orphans.len(), 1);
    assert_eq!(found.orphans[0].service_id, service.id);
    let group = found.orphans[0].process_group_id;
    assert!(process_group_exists(group));

    setup.targets.stop_orphans(&found.target, &found.orphans).await.unwrap();
    assert!(!process_group_exists(group));
    assert!(setup.targets.orphans().is_empty(), "this machine has none");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_reads_only_the_secrets_of_the_service_and_its_dependencies() {
    let setup = setup();
    let db = setup.add(
        &setup.there,
        "Db",
        "echo \"db $DB_PASSWORD\"; sleep 30",
        vec![VariableInput::secret("DB_PASSWORD", "hunter2")],
    );
    let api = setup.add(
        &setup.there,
        "Api",
        "echo \"api $API_TOKEN\"; sleep 30",
        vec![VariableInput::secret("API_TOKEN", "t0ken")],
    );
    let changes = ServiceChanges { dependencies: Some(vec![db.id]), ..Default::default() };
    setup.library.edit_service(api.id, changes, Origin::MainWindow).unwrap();
    let other = setup.add(&setup.there, "Other", "sleep 30", vec![VariableInput::secret("OTHER", "unused")]);
    setup.add(&setup.here, "Local", "sleep 30", vec![VariableInput::secret("LOCAL", "unused")]);
    let (_, mut events) = setup.targets.subscribe();
    setup.take_reads();

    setup.targets.connect(&distro("Test")).await.unwrap();
    assert_eq!(setup.take_reads(), HashSet::new(), "connecting reads no Secret");

    setup.targets.start(api.id, None, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(api.id, ServiceState::Running)).await;
    assert_eq!(setup.take_reads(), HashSet::from([account(&api, "API_TOKEN"), account(&db, "DB_PASSWORD")]));
    output_of(&setup.targets, db.id, "db hunter2").await;
    output_of(&setup.targets, api.id, "api t0ken").await;

    // Edits send the Projects without reading a Secret; the distro keeps the values it has.
    let changes = ServiceChanges { command_line: Some("sleep 31".into()), ..Default::default() };
    setup.library.edit_service(other.id, changes, Origin::MainWindow).unwrap();
    let changes =
        ServiceChanges { variables: Some(vec![VariableInput::secret("API_TOKEN", "n3w")]), ..Default::default() };
    setup.library.edit_service(api.id, changes, Origin::MainWindow).unwrap();
    assert_eq!(setup.take_reads(), HashSet::new());

    // The next start sends the new value.
    setup.targets.restart(api.id, Origin::MainWindow).await.unwrap();
    until(&mut events, reached(api.id, ServiceState::Running)).await;
    output_of(&setup.targets, api.id, "api n3w").await;
    assert_eq!(setup.take_reads(), HashSet::from([account(&api, "API_TOKEN"), account(&db, "DB_PASSWORD")]));
    let db_status = setup.targets.status(db.id);
    assert_eq!(db_status.state, ServiceState::Running);
    assert!(!db_status.is_restart_needed, "the Dependency still has its Secret");

    // Start All reads the Secrets of the Services it starts, not of one it skips.
    let changes = ServiceChanges { is_excluded_from_start_all: Some(true), ..Default::default() };
    setup.library.edit_service(other.id, changes, Origin::MainWindow).unwrap();
    setup.targets.start_all(setup.there.id, Origin::MainWindow).await.unwrap();
    assert_eq!(setup.take_reads(), HashSet::from([account(&api, "API_TOKEN"), account(&db, "DB_PASSWORD")]));
    setup.targets.stop_every_run(Origin::MainWindow).await;
}
