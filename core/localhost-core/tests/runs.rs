//! What a Run does beyond its lifecycle, against real processes. Ported
//! from the Run half of `EnvironmentTests.swift` (what a started Service
//! sees, "Restart to apply"), `ServiceLogTests.swift`, and the parts of
//! `OrphanTests.swift` that need a ServiceManager.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use localhost_core::{
    LocalhostError, NewService, Origin, OutputChannel, OutputLine, ProfileChanges, ProjectChanges, RunRegistry,
    Service, ServiceChanges, ServiceEvent, ServiceManager, ServiceManagerOptions, ServiceState, ServiceStatus,
    StopPolicy, Termination, VariableInput, VariableLayer,
};

// MARK: Environment of a Run

/// A Service that writes the given variables to `env.txt`, then keeps running.
fn printing_service(s: &Services, names: &[&str], variables: Vec<VariableInput>, port: Option<u16>) -> Service {
    let echoes: Vec<String> = names.iter().map(|name| format!("{name}=${name}")).collect();
    let mut web =
        NewService::new("Web", key("web"), format!("echo \"{}\" > env.txt; exec sleep 1000", echoes.join(" ")));
    web.port = port.map(i64::from);
    s.add(web, variables, vec![])
}

/// Starts the Service and returns what it wrote to `env.txt`.
async fn run_output(s: &Services, service: &Service) -> String {
    let file = s.project_folder().join("env.txt");
    let _ = std::fs::remove_file(&file);
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    wait_until(|| std::fs::read_to_string(&file).is_ok_and(|text| text.ends_with('\n'))).await;
    std::fs::read_to_string(&file).unwrap().trim_end().to_string()
}

async fn next_status(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ServiceEvent>,
    service: &Service,
    matches: impl Fn(&ServiceStatus) -> bool,
) -> ServiceStatus {
    statuses_until(events, service.id, matches).await.pop().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_gets_project_then_service_then_active_profile_variables_and_port_over_all() {
    let s = Services::new();
    let project_variables = ["A", "B", "C"].map(|name| VariableInput::plain(name, "project")).to_vec();
    let mut project_variables = project_variables;
    project_variables.push(VariableInput::plain("PORT", "1"));
    s.library
        .edit_project(
            s.project.id,
            ProjectChanges { variables: Some(project_variables), ..Default::default() },
            Origin::MainWindow,
        )
        .unwrap();
    let profile = s
        .library
        .add_profile(s.project.id, "Testing", vec![VariableInput::plain("C", "profile")], Origin::MainWindow)
        .unwrap();
    s.library.set_active_profile(Some(profile.id), s.project.id, Origin::MainWindow).unwrap();
    let port = free_port();
    let service = printing_service(
        &s,
        &["A", "B", "C", "PORT"],
        vec![VariableInput::plain("B", "service"), VariableInput::plain("C", "service")],
        Some(port),
    );

    assert_eq!(run_output(&s, &service).await, format!("A=project B=service C=profile PORT={port}"));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_inactive_profile_adds_nothing() {
    let s = Services::new();
    s.library
        .add_profile(s.project.id, "Testing", vec![VariableInput::plain("C", "profile")], Origin::MainWindow)
        .unwrap();
    let service = printing_service(&s, &["C"], vec![], None);

    assert_eq!(run_output(&s, &service).await, "C=");
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secrets_value_reaches_the_run() {
    let s = Services::new();
    let service = printing_service(
        &s,
        &["TOKEN", "DEBUG"],
        vec![VariableInput::secret("TOKEN", "s3cr3t-value"), VariableInput::plain("DEBUG", "1")],
        None,
    );
    assert_eq!(run_output(&s, &service).await, "TOKEN=s3cr3t-value DEBUG=1");
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_secret_without_a_stored_value_fails_the_start_with_secret_missing() {
    use localhost_core::SecretStore;
    let s = Services::new();
    let service = printing_service(&s, &[], vec![VariableInput::secret("TOKEN", "x")], None);
    s.fixture.secrets.remove_value(&VariableLayer::Service(service.id).secret_account("TOKEN")).unwrap();

    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    assert_eq!(status.run.unwrap().failure, Some(LocalhostError::SecretMissing("TOKEN".into())));
    assert_eq!(s.library.is_secret_set("TOKEN", VariableLayer::Service(service.id)), Ok(false));
}

// MARK: Restart to apply

#[tokio::test(flavor = "multi_thread")]
async fn switching_profiles_marks_the_running_service_and_a_restart_applies_it() {
    let s = Services::new();
    let profile = s
        .library
        .add_profile(s.project.id, "Testing", vec![VariableInput::plain("MODE", "test")], Origin::MainWindow)
        .unwrap();
    let service = printing_service(&s, &["MODE"], vec![], None);
    let (_, mut events) = s.manager.subscribe();
    assert_eq!(run_output(&s, &service).await, "MODE=");

    s.library.set_active_profile(Some(profile.id), s.project.id, Origin::MainWindow).unwrap();
    next_status(&mut events, &service, |status| status.is_restart_needed).await;

    s.manager.restart(service.id, Origin::MainWindow).await.unwrap();
    assert!(!s.manager.status(service.id).is_restart_needed);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    assert_eq!(run_output(&s, &service).await, "MODE=test");
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn changing_a_secrets_value_marks_it_changing_it_back_clears_it_renaming_the_service_doesnt() {
    let s = Services::new();
    let service = printing_service(&s, &["TOKEN"], vec![VariableInput::secret("TOKEN", "a")], None);
    let (_, mut events) = s.manager.subscribe();
    run_output(&s, &service).await;

    let edit = |changes: ServiceChanges| s.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    edit(ServiceChanges { name: Some("Renamed".into()), ..Default::default() });
    edit(ServiceChanges { variables: Some(vec![VariableInput::secret("TOKEN", "b")]), ..Default::default() });
    let marked = next_status(&mut events, &service, |status| status.is_restart_needed).await;
    assert!(marked.state.is_active());

    edit(ServiceChanges { variables: Some(vec![VariableInput::secret("TOKEN", "a")]), ..Default::default() });
    next_status(&mut events, &service, |status| !status.is_restart_needed).await;

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    assert!(!s.manager.status(service.id).is_restart_needed);
}

#[tokio::test(flavor = "multi_thread")]
async fn editing_the_command_line_marks_a_running_service() {
    let s = Services::new();
    let service = printing_service(&s, &[], vec![], None);
    let (_, mut events) = s.manager.subscribe();
    run_output(&s, &service).await;

    let changes = ServiceChanges { command_line: Some("exec sleep 999".into()), ..Default::default() };
    s.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();

    next_status(&mut events, &service, |status| status.is_restart_needed).await;
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn renaming_a_profile_does_not_mark_anything() {
    let s = Services::new();
    let profile = s.library.add_profile(s.project.id, "Testing", vec![], Origin::MainWindow).unwrap();
    s.library.set_active_profile(Some(profile.id), s.project.id, Origin::MainWindow).unwrap();
    let service = printing_service(&s, &[], vec![], None);
    run_output(&s, &service).await;

    let changes = ProfileChanges { name: Some("Renamed".into()), ..Default::default() };
    s.library.edit_profile(profile.id, changes, Origin::MainWindow).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(!s.manager.status(service.id).is_restart_needed);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

// MARK: Logs of a Run

/// Starts a Service and returns its Run's lines once the Run has ended
/// (stopping it after `stop_after`, if given).
async fn run_lines(s: &Services, command_line: &str, stop_after: Option<Duration>) -> Vec<OutputLine> {
    let service = s.service(command_line);
    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    let run_id = status.run.unwrap().id;
    let (_, mut updates) = s.logs.subscribe_to_run(run_id);
    if let Some(delay) = stop_after {
        tokio::time::sleep(delay).await;
        s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    }
    tokio::time::timeout(PATIENCE, async { while updates.recv().await.is_some() {} }).await.unwrap();
    s.logs.lines(run_id)
}

fn on(lines: &[OutputLine], channel: OutputChannel) -> Vec<&str> {
    lines.iter().filter(|line| line.channel == channel).map(|line| line.text.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runs_output_and_exit_are_logged() {
    let s = Services::new();
    let lines = run_lines(&s, "echo out; echo err >&2; sleep 0.3; exit 2", None).await;

    assert_eq!(on(&lines, OutputChannel::Stdout), ["out"]);
    assert_eq!(on(&lines, OutputChannel::Stderr), ["err"]);
    let last = lines.last().unwrap();
    assert_eq!((last.channel, last.text.as_str()), (OutputChannel::Localhost, "Exited with code 2"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_force_kill_is_logged() {
    let options = ServiceManagerOptions {
        stop_policy: StopPolicy {
            interrupt_timeout: Duration::from_millis(300),
            terminate_timeout: Duration::from_millis(300),
        },
        ..test_options()
    };
    let s = Services::with_options(options);
    let lines =
        run_lines(&s, "trap '' INT TERM; echo ready; sleep 1000 & wait", Some(Duration::from_millis(300))).await;

    assert_eq!(
        on(&lines, OutputChannel::Localhost),
        ["Force-killed: the process group did not stop in time", "Terminated by signal SIGKILL"]
    );
}

/// The Service's status once its Run has fully ended.
async fn ended_status(s: &Services) -> ServiceStatus {
    let service_id = s.library.project(s.project.id).unwrap().services[0].id;
    wait_until(|| s.manager.status(service_id).run.is_some_and(|run| run.ended_at.is_some())).await;
    s.manager.status(service_id)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_fails_after_dockers_daemon_error_is_docker_daemon_unavailable() {
    for error in [
        "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
        "failed to connect to the docker API at unix:///Users/me/.docker/run/docker.sock; check if the path is correct and if the daemon is running",
    ] {
        let s = Services::new();
        run_lines(&s, &format!("echo '{error}' >&2; exit 1"), None).await;
        let status = ended_status(&s).await;

        assert_eq!(status.state, ServiceState::Failed);
        let run = status.run.unwrap();
        assert_eq!(run.failure, Some(LocalhostError::DockerDaemonUnavailable));
        assert_eq!(run.termination, Some(Termination::Exited { code: 1 }));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dockers_daemon_error_is_not_a_failure_when_the_run_succeeds() {
    let s = Services::new();
    run_lines(&s, "echo 'Cannot connect to the Docker daemon' >&2; exit 0", None).await;
    let status = ended_status(&s).await;

    assert_eq!(status.state, ServiceState::Exited);
    assert_eq!(status.run.unwrap().failure, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_runs_file_is_written() {
    let s = Services::new();
    let lines = run_lines(&s, "echo persisted", None).await;
    assert_eq!(lines[0].text, "persisted");
    let run_id = ended_status(&s).await.run.unwrap().id;
    let contents = std::fs::read_to_string(s.logs.log_file_path(run_id).unwrap()).unwrap();
    assert!(contents.contains(" stdout persisted\n"));
}

// MARK: Orphans across sessions

fn records(s: &Services) -> Vec<String> {
    std::fs::read_dir(s.fixture.path().join("runs"))
        .map(|entries| {
            entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".json")).collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn records_each_active_run_and_clears_the_record_when_it_ends() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let run = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap();
    assert_eq!(records(&s), [format!("{}.json", run.id)]);

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    assert!(records(&s).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn clears_the_record_when_a_run_ends_on_its_own() {
    let s = Services::new();
    let (_, mut events) = s.manager.subscribe();
    let service = s.service("exit 0");
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    if s.manager.status(service.id).run.unwrap().ended_at.is_none() {
        events_until(&mut events, service.id, is_run_ended).await;
    }
    assert!(records(&s).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_session_finds_the_previous_sessions_surviving_runs_and_never_readopts_them() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let run = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap();

    let registry = Arc::new(RunRegistry::new(s.fixture.path()));
    let orphans = registry.orphans();
    assert_eq!(orphans.iter().map(|o| o.run_id).collect::<Vec<_>>(), [run.id]);
    assert_eq!(orphans[0].service_id, service.id);
    // Recorded as seconds in an f64, as the Swift core did: equal to well within a millisecond.
    let difference =
        orphans[0].started_at.duration_since(run.started_at).or_else(|e| Ok::<_, ()>(e.duration())).unwrap();
    assert!(difference < Duration::from_millis(1), "{difference:?}");

    let new_session = ServiceManager::new(s.library.clone(), s.logs.clone(), registry, test_options());
    assert_eq!(new_session.status(service.id).state, ServiceState::Stopped);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_current_sessions_own_runs_are_not_orphans() {
    let s = Services::new();
    let registry = Arc::new(RunRegistry::new(s.fixture.path()));
    let manager = ServiceManager::new(s.library.clone(), s.logs.clone(), registry.clone(), test_options());
    let service = s.service("sleep 1000");
    manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert!(registry.orphans().is_empty());
    manager.stop(service.id, Origin::MainWindow).await.unwrap();
}
