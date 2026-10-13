//! Ported from `PortTests.swift` and `ListenerTests.swift`: readiness on a
//! Port, `PORT`, detected URLs, Listener attribution, port conflicts and
//! automatic moves, against real processes. Listeners not started by
//! localhost are `nc` process trees launched directly with `ProcessTree`.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use common::*;
use localhost_core::{
    LaunchConfiguration, Listener, ListenerKind, ListenerOwner, LocalhostError, NewService, Origin, PortChange,
    PortChangeReason, ProcessTree, Service, ServiceManagerOptions, ServiceState, StopPolicy, VariableInput,
};

fn port_options(port_timeout: Duration) -> ServiceManagerOptions {
    ServiceManagerOptions { port_timeout, ..test_options() }
}

fn with_port(
    s: &Services,
    raw: &str,
    command_line: &str,
    port: Option<u16>,
    can_change: bool,
    deps: &[&Service],
) -> Service {
    let mut new = NewService::new(raw, key(raw), command_line);
    new.port = port.map(i64::from);
    new.port_can_change = can_change;
    s.add(new, vec![], deps.iter().map(|d| d.id).collect())
}

fn nc_service(s: &Services, raw: &str, port: u16) -> Service {
    with_port(s, raw, "exec nc -lk $PORT", Some(port), true, &[])
}

/// Starts a process tree listening on `port` that localhost doesn't run, in
/// `directory`, and waits until it shows up as a Listener.
async fn listen(s: &Services, port: u16, directory: &Path, command_line: Option<&str>) -> ProcessTree {
    let default = format!("exec nc -lk {port}");
    let configuration = LaunchConfiguration {
        command_line: command_line.unwrap_or(&default).into(),
        working_directory: directory.into(),
        environment: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
        shell: "/bin/sh".into(),
    };
    let quick =
        StopPolicy { interrupt_timeout: Duration::from_millis(500), terminate_timeout: Duration::from_millis(500) };
    let tree = ProcessTree::launch(&configuration, quick).unwrap();
    listener_on(s, port).await;
    tree
}

/// The Listener on `port`, once there is one. Not probed with a connection:
/// `nc` stops listening while it serves one.
async fn listener_on(s: &Services, port: u16) -> Listener {
    let find = async {
        loop {
            if let Some(listener) = s.manager.listeners().await.into_iter().find(|l| l.ports.contains(&port)) {
                return listener;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(PATIENCE, find).await.expect("no Listener appeared")
}

fn outside(s: &Services) -> std::path::PathBuf {
    let folder = s.fixture.path().join("elsewhere");
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

// MARK: Readiness

#[tokio::test(flavor = "multi_thread")]
async fn with_a_port_stays_starting_until_the_port_accepts_connections() {
    let s = Services::new();
    let port = free_port();
    let service = with_port(&s, "web", "sleep 0.6; exec nc -lk $PORT", Some(port), true, &[]);
    let (_, mut events) = s.manager.subscribe();

    let started = Instant::now();
    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    assert_eq!(status.run.unwrap().port, Some(port));
    let seen = statuses_until(&mut events, service.id, |s| s.state == ServiceState::Running).await;

    assert!(started.elapsed() >= Duration::from_millis(600));
    assert_eq!(seen.iter().map(|s| s.state).collect::<Vec<_>>(), [ServiceState::Starting, ServiceState::Running]);
    assert!(!seen.last().unwrap().run.as_ref().unwrap().is_port_unresponsive);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_that_never_opens_times_out_to_running_with_a_warning_cleared_once_it_opens() {
    let s = Services::new();
    let port = free_port();
    // Opens well after the timeout, however slow the machine is.
    let service = with_port(&s, "web", "sleep 3; exec nc -lk $PORT", Some(port), true, &[]);
    let manager = s.manager_with(port_options(Duration::from_millis(800)));
    let (_, mut events) = manager.subscribe();
    manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let seen = statuses_until(&mut events, service.id, |s| {
        s.state == ServiceState::Running && s.run.as_ref().is_some_and(|run| !run.is_port_unresponsive)
    })
    .await;

    assert_eq!(
        seen.iter().map(|s| s.state).collect::<Vec<_>>(),
        [ServiceState::Starting, ServiceState::Running, ServiceState::Running]
    );
    let warnings: Vec<bool> = seen.iter().map(|s| s.run.as_ref().unwrap().is_port_unresponsive).collect();
    assert_eq!(warnings, [false, true, false]);
    manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_listening_on_ipv6_loopback_only_counts_as_accepting() {
    let s = Services::new();
    let port = free_port();
    let service = with_port(&s, "web", "exec nc -6 -lk ::1 $PORT", Some(port), true, &[]);
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let seen = statuses_until(&mut events, service.id, |s| s.state == ServiceState::Running).await;
    assert!(!seen.last().unwrap().run.as_ref().unwrap().is_port_unresponsive);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_command_gets_the_port_as_port_overriding_a_service_variable() {
    let s = Services::new();
    let marker = s.fixture.path().join("port.txt");
    let port = free_port();
    let mut web = NewService::new("Web", key("web"), format!("echo $PORT > '{}'; exec sleep 1000", marker.display()));
    web.port = Some(i64::from(port));
    let service = s.add(web, VariableInput::plain_all([("PORT", "1")]), vec![]);
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    wait_until(|| std::fs::read_to_string(&marker).is_ok_and(|text| text.ends_with('\n'))).await;

    assert_eq!(std::fs::read_to_string(&marker).unwrap(), format!("{port}\n"));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

// MARK: URLs

#[tokio::test(flavor = "multi_thread")]
async fn remembers_the_first_local_url_a_run_prints() {
    let s = Services::new();
    let service = s
        .service(r"printf '\033[32mLocal:\033[0m http://0.0.0.0:5173/\n'; echo http://localhost:9999; exec sleep 1000");
    let (_, mut events) = s.manager.subscribe();
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    let seen =
        statuses_until(&mut events, service.id, |s| s.run.as_ref().is_some_and(|r| r.detected_url.is_some())).await;
    let run = seen.last().unwrap().run.clone().unwrap();
    assert_eq!(run.detected_url.as_deref(), Some("http://localhost:5173/"));
    assert_eq!(service.url_to_open(Some(&run)).as_deref(), Some("http://localhost:5173/"));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

// MARK: Discovery

#[tokio::test(flavor = "multi_thread")]
async fn discovers_a_listeners_command_ports_and_working_directory_unmanaged_outside_every_project() {
    let s = Services::new();
    let port = free_port();
    let folder = outside(&s);
    let tree = listen(&s, port, &folder, None).await;

    let listener = listener_on(&s, port).await;
    assert_eq!(listener.pid, tree.pid());
    assert_eq!(listener.name, "nc");
    assert!(listener.command.ends_with(&format!("-lk {port}")), "{}", listener.command);
    assert_eq!(listener.ports, [port]);
    let resolved = std::fs::canonicalize(&folder).unwrap();
    assert_eq!(listener.working_directory.as_deref(), resolved.to_str());
    assert_eq!(listener.owner, ListenerOwner::Unmanaged);
    assert_eq!(listener.launchd_label, None);
    assert_eq!(listener.kind, ListenerKind::Development);
    tree.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_listener_working_inside_a_projects_folder_belongs_to_that_project() {
    let s = Services::new();
    let port = free_port();
    let subfolder = s.project_folder().join("web");
    std::fs::create_dir_all(&subfolder).unwrap();
    let tree = listen(&s, port, &subfolder, None).await;

    assert_eq!(listener_on(&s, port).await.owner, ListenerOwner::Project { project_id: s.project.id });
    tree.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_listener_of_a_run_belongs_to_that_run() {
    let s = Services::new();
    let port = free_port();
    let service = nc_service(&s, "web", port);
    let (_, mut events) = s.manager.subscribe();
    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    statuses_until(&mut events, service.id, |s| s.state == ServiceState::Running).await;

    let run_id = status.run.unwrap().id;
    assert_eq!(listener_on(&s, port).await.owner, ListenerOwner::Run { service_id: service.id, run_id });
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

// MARK: Stopping

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_runs_listener_stops_its_service() {
    let s = Services::new();
    let port = free_port();
    let service = nc_service(&s, "web", port);
    s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    s.manager.stop_listener(&listener_on(&s, port).await, Origin::MainWindow).await.unwrap();

    let status = s.manager.status(service.id);
    assert_eq!(status.state, ServiceState::Stopped);
    assert_eq!(status.run.unwrap().stop_origin, Some(Origin::MainWindow));
}

#[tokio::test(flavor = "multi_thread")]
async fn stops_an_unmanaged_listener() {
    let s = Services::new();
    let port = free_port();
    let tree = listen(&s, port, &outside(&s), None).await;

    s.manager.stop_listener(&listener_on(&s, port).await, Origin::MainWindow).await.unwrap();

    tokio::time::timeout(PATIENCE, tree.wait_for_exit()).await.unwrap();
    assert!(s.manager.listeners().await.iter().all(|l| !l.ports.contains(&port)));
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_stop_localhost_itself_launchd_jobs_and_docker() {
    let s = Services::new();
    // This test process listens, so it is a Listener to refuse.
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let me = listener_on(&s, port).await;
    assert_eq!(s.manager.stop_listener(&me, Origin::MainWindow).await, Err(LocalhostError::ListenerIsLocalhost));

    let mut job = me.clone();
    job.pid = 1;
    job.launchd_label = Some("homebrew.mxcl.redis".into());
    assert_eq!(
        s.manager.stop_listener(&job, Origin::MainWindow).await,
        Err(LocalhostError::ListenerIsLaunchdJob { label: "homebrew.mxcl.redis".into() })
    );
    let mut docker = me.clone();
    docker.pid = 1;
    docker.kind = ListenerKind::Docker;
    assert_eq!(s.manager.stop_listener(&docker, Origin::MainWindow).await, Err(LocalhostError::ListenerIsDocker));
}

// MARK: Port conflicts

#[tokio::test(flavor = "multi_thread")]
async fn a_taken_port_moves_the_run_to_the_next_free_port_saying_why() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let service = nc_service(&s, "web", port);
    let (_, mut events) = s.manager.subscribe();

    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    let run = status.run.unwrap();
    let moved = run.port.unwrap();
    assert!(moved > port && moved <= port + 100);
    let Some(PortChange { configured_port, reason: PortChangeReason::PortInUse { holder: Some(reported) } }) =
        &run.port_change
    else {
        panic!("expected a move away from the holder, got {:?}", run.port_change)
    };
    assert_eq!(reported.pid, holder.pid());
    assert_eq!(*configured_port, port);
    // The command got the moved port as $PORT.
    let seen = statuses_until(&mut events, service.id, |s| s.state == ServiceState::Running).await;
    assert!(!seen.last().unwrap().run.as_ref().unwrap().is_port_unresponsive);
    // The configuration keeps the Port.
    assert_eq!(s.library.service(service.id).unwrap().1.port, Some(port));
    // The log says why.
    let note = s.logs.lines(run.id).into_iter().find(|line| line.text.starts_with("Port ")).unwrap();
    assert_eq!(
        note.text,
        format!("Port {port} is in use by nc (PID {}): starting on port {moved} instead", holder.pid())
    );

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_automatic_moves_off_a_taken_port_fails_the_start_naming_its_owner() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let service = nc_service(&s, "web", port);
    s.manager.set_automatic_port_move(false);

    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let failure = status.run.unwrap().failure.unwrap();
    let LocalhostError::PortConflict { port: conflict, owner: Some(owner) } = &failure else { panic!("{failure:?}") };
    assert_eq!(*conflict, port);
    assert_eq!(owner.pid, holder.pid());
    assert_eq!(failure.message(), format!("Port {port} is in use by nc (PID {}).", holder.pid()));
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_whose_port_cant_change_is_not_moved() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let service = with_port(&s, "db", "exec nc -lk $PORT", Some(port), false, &[]);

    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let owner = listener_on(&s, port).await;
    assert_eq!(status.run.unwrap().failure, Some(LocalhostError::PortConflict { port, owner: Some(Box::new(owner)) }));
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_with_dependents_is_not_moved() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let api = nc_service(&s, "api", port);
    with_port(&s, "web", "sleep 1000", None, true, &[&api]);

    let status = s.manager.start(api.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let owner = listener_on(&s, port).await;
    assert_eq!(status.run.unwrap().failure, Some(LocalhostError::PortConflict { port, owner: Some(Box::new(owner)) }));
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_held_inside_the_same_project_is_not_moved_away_from() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &s.project_folder(), None).await;
    let service = nc_service(&s, "web", port);

    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();

    assert_eq!(status.state, ServiceState::Failed);
    let Some(LocalhostError::PortConflict { owner: Some(owner), .. }) = status.run.unwrap().failure else { panic!() };
    assert_eq!(owner.owner, ListenerOwner::Project { project_id: s.project.id });
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_move_skips_ports_configured_for_other_services() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let service = nc_service(&s, "web", port);
    with_port(&s, "api", "sleep 1000", Some(port + 1), true, &[]);

    assert!(s.manager.next_free_port(service.id).await.unwrap().is_some_and(|free| free > port + 1));
    let status = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap();
    assert!(status.run.unwrap().port.unwrap() > port + 1);

    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    holder.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn starts_on_a_requested_port_for_that_run_only() {
    let s = Services::new();
    let (port, other) = (free_port(), free_port());
    let service = nc_service(&s, "web", port);

    let status = s.manager.start(service.id, Some(other), Origin::MainWindow).await.unwrap();

    let run = status.run.unwrap();
    assert_eq!(run.port, Some(other));
    assert_eq!(run.port_change, Some(PortChange { configured_port: port, reason: PortChangeReason::Requested }));
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
    let restarted = s.manager.start(service.id, None, Origin::MainWindow).await.unwrap().run.unwrap();
    assert_eq!(restarted.port, Some(port));
    assert_eq!(restarted.port_change, None);
    s.manager.stop(service.id, Origin::MainWindow).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn in_start_all_a_blocked_service_fails_with_port_conflict_and_its_dependents_with_dependency_failed() {
    let s = Services::new();
    let port = free_port();
    let holder = listen(&s, port, &outside(&s), None).await;
    let api = nc_service(&s, "api", port);
    let web = with_port(&s, "web", "sleep 1000", None, true, &[&api]);

    s.manager.start_all(s.project.id, Origin::MainWindow).await.unwrap();
    // Start All returns once every start has taken effect.
    wait_until(|| s.manager.status(web.id).state != ServiceState::Starting).await;

    let api_status = s.manager.status(api.id);
    assert_eq!(api_status.state, ServiceState::Failed);
    assert!(matches!(api_status.run.unwrap().failure, Some(LocalhostError::PortConflict { port: p, .. }) if p == port));
    assert_eq!(s.manager.status(web.id).run.unwrap().failure, Some(LocalhostError::DependencyFailed(api.key.clone())));
    holder.stop().await;
}
