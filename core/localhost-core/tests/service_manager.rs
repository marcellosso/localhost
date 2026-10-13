//! Ported from `ServiceManagerTests.swift`: Service lifecycle against real
//! processes. Each Service's command line is a fixture.

mod common;

use std::time::{Duration, Instant};

use common::*;
use localhost_core::{Id, LocalhostError, NewService, Origin, ServiceEvent, ServiceState, Termination, VariableInput};

fn long_grace() -> localhost_core::ServiceManagerOptions {
    // Tests whose command must end during Starting pass a long grace period:
    // it only bounds a failure, and a busy machine can take longer than 300ms
    // to run even `exit 0`.
    localhost_core::ServiceManagerOptions { grace_period: Duration::from_secs(30), ..test_options() }
}

// MARK: Starting

#[tokio::test(flavor = "multi_thread")]
async fn a_service_that_has_never_run_is_stopped() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let status = s.manager.status(service.id);
    assert_eq!(status.state, ServiceState::Stopped);
    assert_eq!(status.run, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn start_goes_starting_then_running_after_the_grace_period() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let (_, mut events) = s.manager.subscribe();

    let started = Instant::now();
    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    assert_eq!(status.state, ServiceState::Starting);

    let seen = events_until(&mut events, service.id, reached(ServiceState::Running, service.id)).await;
    assert!(started.elapsed() >= Duration::from_millis(300));
    let ServiceEvent::RunStarted(run) = &seen[0] else { panic!("expected runStarted first, got {seen:?}") };
    assert_eq!(run.service_id, service.id);
    assert_eq!(run.origin, Origin::MainWindow);
    assert_eq!(run.ended_at, None);
    assert_eq!(states(&seen), [ServiceState::Starting, ServiceState::Running]);

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn start_is_idempotent_while_starting_or_running() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let first = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    let second = s.manager.start(service.id, None, Origin::MenuBar).await.unwrap();
    assert_eq!(first.run.as_ref().map(|r| r.id), second.run.as_ref().map(|r| r.id));

    tokio::time::sleep(Duration::from_millis(400)).await;
    let third = s.manager.start(service.id, None, Origin::MenuBar).await.unwrap();
    assert_eq!(third.state, ServiceState::Running);
    assert_eq!(third.run.map(|r| r.id), first.run.map(|r| r.id));

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_starts_launch_a_single_run() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let (_, mut events) = s.manager.subscribe();

    let (a, b) = tokio::join!(
        s.manager.start(service.id, None, Origin::MainWindow),
        s.manager.start(service.id, None, Origin::MenuBar)
    );
    assert_eq!(a.unwrap().run.map(|r| r.id), b.unwrap().run.map(|r| r.id));

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    let seen = events_until(&mut events, service.id, is_run_ended).await;
    assert_eq!(seen.iter().filter(|event| matches!(event, ServiceEvent::RunStarted(_))).count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn uses_the_services_working_directory_and_environment() {
    let s = Services::new();
    let folder = s.project_folder().join("web");
    std::fs::create_dir_all(&folder).unwrap();
    let mut web = NewService::new("Web", key("web"), r#"pwd -P > out.txt; echo "$PORT $FORCE_COLOR" >> out.txt"#);
    web.working_directory = "web".into();
    let service = s.add(web, VariableInput::plain_all([("PORT", "3000")]), vec![]);
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, service.id, is_run_ended).await;

    let output = std::fs::read_to_string(folder.join("out.txt")).unwrap();
    assert!(output.ends_with("/project/web\n3000 1\n"), "{output}");
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_an_unknown_service_fails() {
    let s = Services::new();
    let id = Id::new();
    assert_eq!(s.manager.start(id, None, Origin::MainWindow).await, Err(LocalhostError::ServiceNotFound(id)));
}

// MARK: Ending on its own

#[tokio::test(flavor = "multi_thread")]
async fn ending_during_starting_is_exited_on_success_failed_otherwise() {
    for (command_line, expected) in [("exit 0", ServiceState::Exited), ("exit 1", ServiceState::Failed)] {
        let s = Services::with_options(long_grace());
        let service = s.service(command_line);
        let (_, mut events) = s.manager.subscribe();
        s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

        let seen = events_until(&mut events, service.id, is_run_ended).await;
        assert_eq!(states(&seen), [ServiceState::Starting, expected], "{command_line}");
        assert_eq!(s.manager.status(service.id).state, expected);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ending_on_its_own_after_running_is_exited_on_success_failed_otherwise() {
    for (command_line, expected_state, expected_termination) in [
        ("sleep 0.5; exit 0", ServiceState::Exited, Termination::Exited { code: 0 }),
        ("sleep 0.5; exit 1", ServiceState::Failed, Termination::Exited { code: 1 }),
        ("sleep 0.5; kill -KILL $$", ServiceState::Failed, Termination::Signaled { signal: libc::SIGKILL }),
    ] {
        let s = Services::new();
        let service = s.service(command_line);
        let (_, mut events) = s.manager.subscribe();
        s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

        let seen = events_until(&mut events, service.id, is_run_ended).await;
        assert_eq!(states(&seen), [ServiceState::Starting, ServiceState::Running, expected_state], "{command_line}");
        let Some(ServiceEvent::RunEnded(run)) = seen.last() else { panic!() };
        assert_eq!(run.termination, Some(expected_termination));
        assert_eq!(run.failure, None);
        assert_eq!(run.stop_origin, None);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_command_is_failed_with_command_not_found() {
    let s = Services::new();
    let service = s.service(&format!("definitely-not-a-command-{}", Id::new().to_string()[..8].to_lowercase()));
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let seen = events_until(&mut events, service.id, is_run_ended).await;
    let Some(ServiceEvent::RunEnded(run)) = seen.last() else { panic!() };
    assert_eq!(run.termination, Some(Termination::Exited { code: 127 }));
    assert_eq!(run.failure, Some(LocalhostError::CommandNotFound));
    assert_eq!(s.manager.status(service.id).state, ServiceState::Failed);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_failure_is_failed_with_a_run_that_records_the_error() {
    let s = Services::new();
    let mut missing = NewService::new("Svc", unique_key("svc"), "sleep 1000");
    missing.working_directory = "missing".into();
    let service = s.add(missing, vec![], vec![]);

    let status = s.manager.start(service.id, None, Origin::MenuBar).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let run = status.run.unwrap();
    assert_eq!(run.failure, Some(LocalhostError::WorkingDirectoryMissing(format!("{}/missing", s.project.path))));
    assert_eq!(run.termination, None);
    assert_eq!(run.origin, Origin::MenuBar);
    assert!(run.ended_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn never_running_once_the_shell_has_exited_even_while_leftovers_are_being_cleaned_up() {
    let s = Services::new();
    let service = s.service(&leftover_ignoring_signals("sleep 0.5; exit 1"));
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, service.id, reached(ServiceState::Running, service.id)).await;

    events_until(&mut events, service.id, reached(ServiceState::Failed, service.id)).await;
    let status = s.manager.status(service.id);
    assert_eq!(status.state, ServiceState::Failed);
    // The Run is still ending: its leftover ignores SIGTERM and needs SIGKILL.
    assert_eq!(status.run.unwrap().ended_at, None);

    let seen = events_until(&mut events, service.id, is_run_ended).await;
    let Some(ServiceEvent::RunEnded(run)) = seen.last() else { panic!() };
    assert!(run.was_force_killed);
    assert_eq!(s.manager.status(service.id).state, ServiceState::Failed);
}

// MARK: Stopping

#[tokio::test(flavor = "multi_thread")]
async fn a_requested_stop_ends_stopped_whatever_the_exit_code() {
    let s = Services::new();
    let service = s.service("trap 'exit 3' INT; while :; do sleep 0.05; done");
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, service.id, reached(ServiceState::Running, service.id)).await;

    let status = s.manager.stop(service.id, Origin::MenuBar).await.unwrap();

    assert_eq!(status.state, ServiceState::Stopped);
    let run = status.run.unwrap();
    assert_eq!(run.termination, Some(Termination::Exited { code: 3 }));
    assert_eq!(run.origin, Origin::MainWindow);
    assert_eq!(run.stop_origin, Some(Origin::MenuBar));
    assert!(run.started_at <= run.ended_at.unwrap());
    let seen = events_until(&mut events, service.id, is_run_ended).await;
    assert_eq!(states(&seen), [ServiceState::Stopping, ServiceState::Stopped]);
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_during_starting_ends_stopped() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    assert_eq!(s.manager.stop(service.id, Origin::MainWindow).await.unwrap().state, ServiceState::Stopped);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_is_a_no_op_when_nothing_is_running() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    assert_eq!(s.manager.stop(service.id, Origin::MainWindow).await.unwrap().state, ServiceState::Stopped);

    let exited = s.service("exit 1");
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(exited.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, exited.id, is_run_ended).await;
    assert_eq!(s.manager.stop(exited.id, Origin::MainWindow).await.unwrap().state, ServiceState::Failed);
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_stops_both_return_stopped() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let (a, b) =
        tokio::join!(s.manager.stop(service.id, Origin::MainWindow), s.manager.stop(service.id, Origin::MenuBar));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.state, ServiceState::Stopped);
    assert_eq!(b.state, ServiceState::Stopped);
    assert_eq!(a.run.map(|r| r.id), b.run.map(|r| r.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_while_stopping_waits_for_the_stop_then_starts_a_new_run() {
    let s = Services::new();
    let service = s.service("trap '' INT; while :; do sleep 0.05; done");
    let (_, mut events) = s.manager.subscribe();
    let first = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, service.id, reached(ServiceState::Running, service.id)).await;

    let manager = s.manager.clone();
    let stopped = tokio::spawn(async move { manager.stop(service.id, Origin::MainWindow).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(s.manager.status(service.id).state, ServiceState::Stopping);
    let restarted = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(stopped.await.unwrap().unwrap().state, ServiceState::Stopped);
    assert_eq!(restarted.state, ServiceState::Starting);
    assert_ne!(restarted.run.map(|r| r.id), first.run.map(|r| r.id));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

// MARK: Restarting

#[tokio::test(flavor = "multi_thread")]
async fn restart_stops_the_current_run_and_starts_a_new_one() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let first = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    let (_, mut events) = s.manager.subscribe();

    let restarted = s.manager.restart(service.id, Origin::MenuBar).await.unwrap();

    assert_eq!(restarted.state, ServiceState::Starting);
    let run = restarted.run.unwrap();
    assert_ne!(Some(run.id), first.run.map(|r| r.id));
    assert_eq!(run.origin, Origin::MenuBar);
    let seen = events_until(&mut events, service.id, reached(ServiceState::Running, service.id)).await;
    assert_eq!(
        states(&seen),
        [ServiceState::Stopping, ServiceState::Stopped, ServiceState::Starting, ServiceState::Running]
    );
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_starts_a_service_that_is_not_running() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    assert_eq!(s.manager.restart(service.id, Origin::MainWindow).await.unwrap().state, ServiceState::Starting);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn each_start_reads_the_current_service_definition() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    let changes = localhost_core::ServiceChanges { command_line: Some("exit 7".into()), ..Default::default() };
    s.library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let seen = events_until(&mut events, service.id, is_run_ended).await;
    let Some(ServiceEvent::RunEnded(run)) = seen.last() else { panic!() };
    assert_eq!(run.termination, Some(Termination::Exited { code: 7 }));
}

// MARK: Stopping everything

#[tokio::test(flavor = "multi_thread")]
async fn stop_every_run_stops_every_live_run_in_parallel_and_leaves_the_rest_alone() {
    let s = Services::new();
    let stubborn = s.service("trap '' INT; while :; do sleep 0.05; done");
    let plain = s.service("trap '' INT; while :; do sleep 0.05; done");
    let idle = s.service("sleep 1000");
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(stubborn.id, None, Origin::MainWindow).await.unwrap();
    s.manager.start(plain.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, plain.id, reached(ServiceState::Running, plain.id)).await;

    let started = Instant::now();
    s.manager.stop_every_run(Origin::MainWindow).await;

    assert_eq!(s.manager.status(stubborn.id).state, ServiceState::Stopped);
    assert_eq!(s.manager.status(plain.id).state, ServiceState::Stopped);
    assert_eq!(s.manager.status(idle.id).state, ServiceState::Stopped);
    assert_eq!(s.manager.status(idle.id).run, None);
    // Both needed SIGTERM after a 500 ms SIGINT timeout; in sequence it would take twice as long.
    assert!(started.elapsed() < Duration::from_millis(900), "{:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_every_run_also_ends_runs_whose_leftovers_are_still_being_cleaned_up() {
    let s = Services::new();
    let service = s.service(&leftover_ignoring_signals("sleep 0.4; exit 1"));
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    events_until(&mut events, service.id, reached(ServiceState::Failed, service.id)).await;
    assert_eq!(s.manager.status(service.id).run.unwrap().ended_at, None);

    s.manager.stop_every_run(Origin::MainWindow).await;

    let status = s.manager.status(service.id);
    assert_eq!(status.state, ServiceState::Failed);
    assert!(status.run.unwrap().ended_at.is_some());
}

// MARK: Observing

#[tokio::test(flavor = "multi_thread")]
async fn subscribe_returns_the_current_snapshot_with_the_stream() {
    let s = Services::new();
    let service = s.service("sleep 1000");
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let (snapshot, _) = s.manager.subscribe();
    assert_eq!(snapshot.get(&service.id).map(|status| status.state), Some(ServiceState::Starting));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}
