//! Ported from `ProjectControlTests.swift`: Dependencies, Start All and Stop
//! All against real processes.

mod common;

use std::time::Duration;

use common::*;
use localhost_core::{LocalhostError, NewService, Origin, Service, ServiceEvent, ServiceManagerOptions, ServiceState};

fn long_grace() -> ServiceManagerOptions {
    ServiceManagerOptions { grace_period: Duration::from_secs(30), ..test_options() }
}

fn service(s: &Services, raw: &str, command_line: &str, dependencies: &[&Service], excluded: bool) -> Service {
    let mut new = NewService::new(raw, key(raw), command_line);
    new.is_excluded_from_start_all = excluded;
    s.add(new, vec![], dependencies.iter().map(|d| d.id).collect())
}

fn sleeper(s: &Services, raw: &str, dependencies: &[&Service]) -> Service {
    service(s, raw, "sleep 1000", dependencies, false)
}

fn index(events: &[ServiceEvent], predicate: impl Fn(&ServiceEvent) -> bool) -> Option<usize> {
    events.iter().position(predicate)
}

// MARK: Starting with Dependencies

#[tokio::test(flavor = "multi_thread")]
async fn starting_a_service_starts_its_dependency_first_and_waits_until_it_is_running() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();

    let status = s.manager.start(api.id, None, Origin::MainWindow).await.unwrap();
    assert_eq!(status.state, ServiceState::Starting);
    assert!(status.run.is_some());
    assert!(status.awaited_dependencies.is_empty());

    let seen = all_events_until(&mut stream, run_started(api.id)).await;
    let waiting = seen.iter().any(|event| {
        matches!(event, ServiceEvent::ServiceStateChanged(status)
            if status.service_id == api.id && status.state == ServiceState::Starting && status.awaited_dependencies == [database.id])
    });
    assert!(waiting);
    let database_running = index(&seen, reached(ServiceState::Running, database.id)).unwrap();
    let api_launched = index(&seen, run_started(api.id)).unwrap();
    assert!(database_running < api_launched);
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dependency_that_exits_satisfies_its_dependents() {
    let s = Services::with_options(long_grace());
    let migrate = service(&s, "migrate", "exit 0", &[], false);
    let api = sleeper(&s, "api", &[&migrate]);

    let status = s.manager.start(api.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Starting);
    assert!(status.run.is_some());
    assert_eq!(s.manager.status(migrate.id).state, ServiceState::Exited);
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_dependency_leaves_its_dependents_failed_with_dependency_failed() {
    let s = Services::with_options(long_grace());
    let database = service(&s, "database", "exit 1", &[], false);
    let api = sleeper(&s, "api", &[&database]);
    let web = sleeper(&s, "web", &[&api]);

    let status = s.manager.start(web.id, None, Origin::MenuBar).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let run = status.run.unwrap();
    assert_eq!(run.failure, Some(LocalhostError::DependencyFailed(key("api"))));
    assert_eq!(run.termination, None);
    assert_eq!(run.origin, Origin::MenuBar);
    assert_eq!(s.manager.status(api.id).run.unwrap().failure, Some(LocalhostError::DependencyFailed(key("database"))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dependency_that_is_already_running_is_not_restarted() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();
    let first = s.manager.start(database.id, None, Origin::MainWindow).await.unwrap();
    all_events_until(&mut stream, reached(ServiceState::Running, database.id)).await;

    s.manager.start(api.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(s.manager.status(database.id).run.map(|r| r.id), first.run.map(|r| r.id));
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_service_waiting_for_its_dependency_ends_it_stopped_and_lets_the_dependency_start() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();

    let manager = s.manager.clone();
    let started = tokio::spawn(async move { manager.start(api.id, None, Origin::MainWindow).await });
    all_events_until(&mut stream, run_started(database.id)).await;
    let stopped = s.manager.stop(api.id, Origin::MainWindow).await.unwrap();

    assert_eq!(stopped.state, ServiceState::Stopped);
    assert_eq!(stopped.run, None);
    assert_eq!(started.await.unwrap().unwrap().state, ServiceState::Stopped);
    all_events_until(&mut stream, reached(ServiceState::Running, database.id)).await;
    assert_eq!(s.manager.status(api.id).run, None);
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_dependency_others_wait_on_ends_them_stopped() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();

    let manager = s.manager.clone();
    let started = tokio::spawn(async move { manager.start(api.id, None, Origin::MainWindow).await });
    all_events_until(&mut stream, run_started(database.id)).await;
    s.manager.stop(database.id, Origin::MainWindow).await.unwrap();

    let status = started.await.unwrap().unwrap();
    assert_eq!(status.state, ServiceState::Stopped);
    assert_eq!(status.run, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_dependency_leaves_its_dependents_running() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();
    let api_run = s.manager.start(api.id, None, Origin::MainWindow).await.unwrap().run;
    all_events_until(&mut stream, reached(ServiceState::Running, api.id)).await;

    s.manager.restart(database.id, Origin::MainWindow).await.unwrap();

    let status = s.manager.status(api.id);
    assert_eq!(status.state, ServiceState::Running);
    assert_eq!(status.run.map(|r| r.id), api_run.map(|r| r.id));
    s.manager.stop_every_run(Origin::MainWindow).await;
}

// MARK: Start All and Stop All

#[tokio::test(flavor = "multi_thread")]
async fn start_all_starts_every_included_service_after_its_dependencies() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let migrate = service(&s, "migrate", "exit 0", &[], true);
    let api = sleeper(&s, "api", &[&database, &migrate]);
    let web = sleeper(&s, "web", &[&api]);
    let seed = service(&s, "seed", "exit 0", &[], true);
    let (_, mut stream) = s.manager.subscribe();

    s.manager.start_all(s.project.id, Origin::MenuBar).await.unwrap();

    let seen = all_events_until(&mut stream, run_started(web.id)).await;
    let order: Vec<usize> =
        [&database, &migrate, &api, &web].iter().map(|s| index(&seen, run_started(s.id)).unwrap()).collect();
    assert!(order[0] < order[2] && order[1] < order[2] && order[2] < order[3], "{order:?}");
    assert_eq!(s.manager.status(seed.id).run, None);
    assert_eq!(s.manager.status(web.id).run.map(|r| r.origin), Some(Origin::MenuBar));
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_all_runs_an_exited_service_again_before_its_dependents() {
    let s = Services::with_options(long_grace());
    let migrate = service(&s, "migrate", "exit 0", &[], false);
    let api = sleeper(&s, "api", &[&migrate]);
    s.manager.start_all(s.project.id, Origin::MainWindow).await.unwrap();
    s.manager.stop_all(s.project.id, Origin::MainWindow).await.unwrap();
    assert_eq!(s.manager.status(migrate.id).state, ServiceState::Exited);
    let (_, mut stream) = s.manager.subscribe();

    s.manager.start_all(s.project.id, Origin::MainWindow).await.unwrap();

    let seen = all_events_until(&mut stream, run_started(api.id)).await;
    assert!(index(&seen, run_started(migrate.id)).is_some());
    s.manager.stop_every_run(Origin::MainWindow).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_all_stops_dependents_before_their_dependencies() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let web = sleeper(&s, "web", &[&database]);
    let (_, mut stream) = s.manager.subscribe();
    s.manager.start_all(s.project.id, Origin::MainWindow).await.unwrap();
    all_events_until(&mut stream, reached(ServiceState::Running, web.id)).await;

    s.manager.stop_all(s.project.id, Origin::MainWindow).await.unwrap();

    let seen = all_events_until(&mut stream, reached(ServiceState::Stopped, database.id)).await;
    let web_stopped = index(&seen, reached(ServiceState::Stopped, web.id)).unwrap();
    let database_stopping = index(&seen, reached(ServiceState::Stopping, database.id)).unwrap();
    assert!(web_stopped < database_stopping);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_all_cancels_starts_still_waiting_for_their_dependencies() {
    let s = Services::new();
    let database = sleeper(&s, "database", &[]);
    let api = sleeper(&s, "api", &[&database]);
    let (_, mut stream) = s.manager.subscribe();

    let (manager, project_id) = (s.manager.clone(), s.project.id);
    let started_all = tokio::spawn(async move { manager.start_all(project_id, Origin::MainWindow).await });
    all_events_until(&mut stream, run_started(database.id)).await;
    s.manager.stop_all(s.project.id, Origin::MainWindow).await.unwrap();
    started_all.await.unwrap().unwrap();

    assert_eq!(s.manager.status(api.id).state, ServiceState::Stopped);
    assert_eq!(s.manager.status(api.id).run, None);
    assert_eq!(s.manager.status(database.id).state, ServiceState::Stopped);
}
