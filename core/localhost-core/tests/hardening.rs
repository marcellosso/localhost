//! Ported from `HardeningTests.swift`: lifecycle edge cases against real
//! processes. Storms of starts and stops, restarts during Starting, output
//! floods, huge lines and bytes that aren't UTF-8. Each command line ends
//! with a unique marker so a test can check that nothing it launched is left
//! running.

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use localhost_core::{
    Id, LogStore, MAX_LINE_LENGTH, NewService, Origin, RunRegistry, Service, ServiceEvent, ServiceManager,
    ServiceManagerOptions, ServiceState, StopPolicy,
};

struct Hardening {
    s: Services,
    logs: Arc<LogStore>,
    manager: ServiceManager,
    marker: String,
}

impl Hardening {
    fn new() -> Hardening {
        let s = Services::new();
        // Small, so the bounds are reached quickly.
        let logs = Arc::new(LogStore::with_limits(s.fixture.path(), 1_000, 1_000_000, 5));
        let options = ServiceManagerOptions {
            // Long enough that a command ending on its own does so within Starting on a busy machine.
            grace_period: Duration::from_secs(5),
            stop_policy: StopPolicy {
                interrupt_timeout: Duration::from_millis(500),
                terminate_timeout: Duration::from_millis(500),
            },
            ..test_options()
        };
        let manager =
            ServiceManager::new(s.library.clone(), logs.clone(), Arc::new(RunRegistry::new(s.fixture.path())), options);
        Hardening { s, logs, manager, marker: format!("hardening-{}", Id::new()) }
    }

    /// A Service running `command_line`, tagged with the test's marker.
    fn service(&self, command_line: &str, port: Option<u16>, dependencies: Vec<Id>) -> Service {
        let mut new = NewService::new("Service", unique_key("svc"), format!("{command_line}; : {}", self.marker));
        new.port = port.map(i64::from);
        new.port_can_change = false;
        self.s.add(new, vec![], dependencies)
    }

    /// The pids of processes this test launched that are still alive.
    fn leftovers(&self) -> Vec<String> {
        let output = std::process::Command::new("pgrep").args(["-f", &self.marker]).output().unwrap();
        String::from_utf8_lossy(&output.stdout).lines().map(String::from).collect()
    }

    /// The Run records left in the registry (an Orphan candidate each).
    fn run_records(&self) -> usize {
        std::fs::read_dir(self.s.fixture.path().join("runs")).map(|entries| entries.count()).unwrap_or(0)
    }

    fn log_file_size(&self, run_id: Id) -> u64 {
        let path = self.logs.log_file_path(run_id).unwrap();
        let backup = format!("{}.1", path.display());
        [path.to_string_lossy().into_owned(), backup].iter().map(|p| std::fs::metadata(p).map_or(0, |m| m.len())).sum()
    }
}

// MARK: Rapid start and stop

#[tokio::test(flavor = "multi_thread")]
async fn a_storm_of_concurrent_starts_stops_and_restarts_runs_one_run_at_a_time_and_leaves_nothing_behind() {
    let h = Hardening::new();
    let service = h.service("sleep 1000", None, vec![]);
    let (_, mut events) = h.manager.subscribe();

    let mut storm = tokio::task::JoinSet::new();
    for index in 0..45 {
        let manager = h.manager.clone();
        storm.spawn(async move {
            match index % 3 {
                0 => drop(manager.start(service.id, None, Origin::MainWindow).await),
                1 => drop(manager.stop(service.id, Origin::MenuBar).await),
                _ => drop(manager.restart(service.id, Origin::MainWindow).await),
            }
        });
    }
    tokio::time::timeout(PATIENCE * 2, storm.join_all()).await.unwrap();
    h.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut active, mut max_active) = (HashSet::new(), 0);
    while let Ok(event) = events.try_recv() {
        match event {
            ServiceEvent::RunStarted(run) => drop(active.insert(run.id)),
            ServiceEvent::RunEnded(run) => drop(active.remove(&run.id)),
            ServiceEvent::ServiceStateChanged(_) => {}
        }
        max_active = max_active.max(active.len());
    }
    assert!(max_active <= 1, "{max_active} Runs at once");
    assert_eq!(h.manager.status(service.id).state, ServiceState::Stopped);
    assert!(h.leftovers().is_empty());
    assert_eq!(h.run_records(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_and_stopping_at_once_again_and_again_always_ends_stopped() {
    let h = Hardening::new();
    let service = h.service("sleep 1000", None, vec![]);

    for _ in 0..20 {
        h.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
        let status = h.manager.stop(service.id, Origin::MainWindow).await.unwrap();
        assert_eq!(status.state, ServiceState::Stopped);
    }

    assert!(h.leftovers().is_empty());
    assert_eq!(h.run_records(), 0);
}

// MARK: Restart during Starting

#[tokio::test(flavor = "multi_thread")]
async fn restarting_during_starting_replaces_the_run_and_its_whole_tree() {
    let h = Hardening::new();
    // The Port never opens, so the Run stays Starting.
    let service = h.service("sleep 1000", Some(free_port()), vec![]);

    let first = h.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    let second = h.manager.restart(service.id, Origin::MainWindow).await.unwrap();

    assert_eq!(first.state, ServiceState::Starting);
    assert_eq!(second.state, ServiceState::Starting);
    assert_ne!(second.run.map(|r| r.id), first.run.map(|r| r.id));
    // Only the new Run's shell is alive.
    assert_eq!(h.leftovers().len(), 1);

    h.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    assert!(h.leftovers().is_empty());
    assert_eq!(h.run_records(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn restarting_a_service_still_waiting_for_its_dependencies_then_stopping_all_leaves_nothing_behind() {
    let h = Hardening::new();
    let database = h.service("sleep 1000", Some(free_port()), vec![]);
    let api = h.service("sleep 1000", None, vec![database.id]);
    let project_id = h.s.project.id;

    // `start` returns once the Dependency is satisfied, which takes the port timeout here.
    let manager = h.manager.clone();
    let start = tokio::spawn(async move { manager.start(api.id, None, Origin::MainWindow).await });
    wait_until(|| h.manager.status(database.id).run.is_some()).await;
    let manager = h.manager.clone();
    let restart = tokio::spawn(async move { manager.restart(api.id, Origin::MainWindow).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stopping = Instant::now();
    h.manager.stop_all(project_id, Origin::MainWindow).await.unwrap();
    start.await.unwrap().unwrap();
    restart.await.unwrap().unwrap();
    h.manager.stop_all(project_id, Origin::MainWindow).await.unwrap();

    // Nothing waits for the Port that never opens.
    assert!(stopping.elapsed() < Duration::from_secs(10));
    assert_eq!(h.manager.status(api.id).state, ServiceState::Stopped);
    assert_eq!(h.manager.status(database.id).state, ServiceState::Stopped);
    assert!(h.leftovers().is_empty());
    assert_eq!(h.run_records(), 0);
}

// MARK: Output

#[tokio::test(flavor = "multi_thread")]
async fn a_command_flooding_its_output_stays_bounded_and_still_stops_promptly() {
    let h = Hardening::new();
    let service = h.service("yes 'a chatty line of output'", None, vec![]);

    let run_id = h.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap().id;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let started = Instant::now();
    let stopped = h.manager.stop(service.id, Origin::MainWindow).await.unwrap();

    assert_eq!(stopped.state, ServiceState::Stopped);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!stopped.run.unwrap().was_force_killed);
    assert_eq!(h.logs.lines(run_id).len(), 1_000);
    assert!(h.log_file_size(run_id) <= 1_000_000);
    assert!(h.leftovers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_of_megabytes_is_kept_in_pieces_and_its_log_file_stays_within_the_cap() {
    let h = Hardening::new();
    let service = h.service(r"head -c 3000000 /dev/zero | tr '\0' x; echo", None, vec![]);

    let run_id = h.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap().id;
    let (_, mut updates) = h.logs.subscribe_to_run(run_id);
    tokio::time::timeout(PATIENCE, async { while updates.recv().await.is_some() {} }).await.unwrap();

    let pieces: Vec<String> = h
        .logs
        .lines(run_id)
        .into_iter()
        .filter(|line| line.channel == localhost_core::OutputChannel::Stdout)
        .map(|line| line.text)
        .collect();
    assert!(pieces.iter().all(|piece| piece.len() <= MAX_LINE_LENGTH));
    assert_eq!(pieces.concat(), "x".repeat(3_000_000));
    assert!(h.log_file_size(run_id) <= 1_000_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn output_that_isnt_utf8_is_shown_with_replacement_characters_and_still_read_for_urls() {
    let h = Hardening::new();
    let service = h.service(r"printf 'caf\351 on http://localhost:4321/\nnul\000byte\n'; sleep 1000", None, vec![]);

    let (_, mut events) = h.manager.subscribe();
    let run_id = h.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap().id;
    statuses_until(&mut events, service.id, |s| s.run.as_ref().is_some_and(|r| r.detected_url.is_some())).await;

    assert_eq!(h.manager.status(service.id).run.unwrap().detected_url.as_deref(), Some("http://localhost:4321/"));
    let texts: Vec<String> = h.logs.lines(run_id).into_iter().map(|line| line.text).collect();
    assert_eq!(texts, ["caf\u{FFFD} on http://localhost:4321/", "nul\0byte"]);
    h.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    // The log file is valid UTF-8 text.
    let contents = std::fs::read_to_string(h.logs.log_file_path(run_id).unwrap()).unwrap();
    assert!(contents.contains("caf\u{FFFD} on"));
}
