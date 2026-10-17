//! What a Run's process tree uses (ADR-0012), against real processes: a busy
//! loop for CPU, and on Unix a child that left the Run's session.

mod common;

use std::time::Duration;

use common::*;
use localhost_core::{Id, LocalhostError, Origin, RunStats, ServiceState};

/// A shell that sleeps, a busy loop, and a child that leaves the session
/// with `setsid` (and ends once its parent has).
#[cfg(unix)]
const BUSY: &str = "sh -c 'while :; do :; done' & \
    perl -e 'use POSIX; POSIX::setsid(); $p = getppid(); sleep 1 while getppid() == $p' & \
    exec sleep 1000";
/// The sleeping shell, the busy loop and the `setsid` child.
#[cfg(unix)]
const BUSY_PROCESSES: u32 = 3;

/// `cmd.exe` and a busy loop in its job.
#[cfg(windows)]
const BUSY: &str = r#"python -c "while True: pass""#;
#[cfg(windows)]
const BUSY_PROCESSES: u32 = 2;

async fn stats(s: &Services, service_id: Id, previous: Option<RunStats>) -> RunStats {
    s.manager.run_stats(service_id, previous).await.unwrap().expect("the Run is active")
}

#[tokio::test(flavor = "multi_thread")]
async fn adds_up_the_cpu_memory_and_processes_of_a_runs_tree() {
    let s = Services::new();
    let service = s.service(BUSY);
    let (_, mut events) = s.manager.subscribe();
    let run_id = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap().id;
    statuses_until(&mut events, service.id, |s| s.state == ServiceState::Running).await;

    let mut first = stats(&s, service.id, None).await;
    let started = std::time::Instant::now();
    while first.process_count < BUSY_PROCESSES && started.elapsed() < PATIENCE {
        tokio::time::sleep(Duration::from_millis(50)).await;
        first = stats(&s, service.id, None).await;
    }
    assert_eq!(first.run_id, run_id);
    assert_eq!(first.process_count, BUSY_PROCESSES);
    assert!(first.memory_bytes > 0, "{first:?}");
    assert_eq!(first.cpu_percent, None);

    tokio::time::sleep(Duration::from_millis(500)).await;
    let second = stats(&s, service.id, Some(first.clone())).await;
    assert!(second.cpu_time_microseconds > first.cpu_time_microseconds, "{first:?} → {second:?}");
    // One busy core, give or take a loaded machine.
    let cpu_percent = second.cpu_percent.unwrap();
    assert!((20.0..=200.0).contains(&cpu_percent), "{second:?}");

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    assert_eq!(s.manager.run_stats(service.id, Some(second)).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_without_an_active_run_has_no_stats() {
    let s = Services::new();
    let service = s.service(BUSY);
    assert_eq!(s.manager.run_stats(service.id, None).await.unwrap(), None);

    let unknown = Id::new();
    assert_eq!(s.manager.run_stats(unknown, None).await, Err(LocalhostError::ServiceNotFound(unknown)));
}
