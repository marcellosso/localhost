#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use localhost_core::{Key, MemorySecretStore, ProjectLibrary};
use tempfile::TempDir;

/// A storage root in a temporary folder, removed when dropped, with Secrets
/// kept in memory so tests never touch the user's real store.
pub struct Fixture {
    pub root: TempDir,
    pub secrets: Arc<MemorySecretStore>,
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture { root: tempfile::tempdir().unwrap(), secrets: Arc::new(MemorySecretStore::new()) }
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn library(&self) -> ProjectLibrary {
        ProjectLibrary::open(self.path(), self.secrets.clone()).unwrap()
    }

    pub fn projects_directory(&self) -> PathBuf {
        self.path().join("projects")
    }

    pub fn project_file(&self, id: localhost_core::Id) -> PathBuf {
        self.projects_directory().join(format!("{id}.json"))
    }

    /// Writes a file into the projects folder, as a crash or another tool might.
    pub fn write_project_file(&self, name: &str, contents: &str) -> PathBuf {
        std::fs::create_dir_all(self.projects_directory()).unwrap();
        let path = self.projects_directory().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

pub fn key(raw: &str) -> Key {
    Key::new(raw).unwrap()
}

/// An absolute Project path on this platform, for Projects nothing runs in.
pub fn some_project_path(name: &str) -> String {
    if cfg!(windows) { format!(r"C:\{name}") } else { format!("/tmp/{name}") }
}

// MARK: Running Services

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc as StdArc, Mutex};
use std::time::Duration;

use localhost_core::{
    Id, LogStore, NewService, Origin, Project, RunRegistry, Service, ServiceEvent, ServiceManager,
    ServiceManagerOptions, ServiceState, ServiceStatus, StopPolicy, VariableInput,
};
use tokio::sync::mpsc::UnboundedReceiver;

/// How long a test waits for something before failing instead of hanging.
pub const PATIENCE: Duration = Duration::from_secs(20);

/// A small, predictable environment; on Windows this process's own, so
/// `python` resolves as it does for the test runner.
pub fn base_environment() -> BTreeMap<String, String> {
    if cfg!(windows) {
        return std::env::vars().collect();
    }
    [("PATH", "/usr/bin:/bin"), ("HOME", std::env::var("HOME").unwrap_or_default().as_str()), ("SHELL", "/bin/sh")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Options for tests: a short grace period, quick stops.
pub fn test_options() -> ServiceManagerOptions {
    ServiceManagerOptions {
        base_environment: base_environment(),
        grace_period: Duration::from_millis(300),
        port_timeout: Duration::from_secs(30),
        port_probe_interval: Duration::from_millis(50),
        stop_policy: StopPolicy {
            interrupt_timeout: Duration::from_millis(500),
            terminate_timeout: Duration::from_millis(500),
        },
        automatic_port_move: true,
    }
}

/// A library with one Project whose folder exists, and a manager running it.
pub struct Services {
    pub fixture: Fixture,
    pub library: StdArc<ProjectLibrary>,
    pub logs: StdArc<LogStore>,
    pub manager: ServiceManager,
    pub project: Project,
}

impl Services {
    pub fn new() -> Services {
        Services::with_options(test_options())
    }

    pub fn with_options(options: ServiceManagerOptions) -> Services {
        let fixture = Fixture::new();
        let library = StdArc::new(fixture.library());
        let logs = StdArc::new(LogStore::new(fixture.path()));
        let folder = fixture.path().join("project");
        std::fs::create_dir_all(&folder).unwrap();
        let project =
            library.add_project("App", key("app"), folder.to_str().unwrap(), vec![], Origin::MainWindow).unwrap();
        let manager =
            ServiceManager::new(library.clone(), logs.clone(), StdArc::new(RunRegistry::new(fixture.path())), options);
        Services { fixture, library, logs, manager, project }
    }

    /// Another manager on the same library and storage, e.g. with other options.
    pub fn manager_with(&self, options: ServiceManagerOptions) -> ServiceManager {
        ServiceManager::new(
            self.library.clone(),
            self.logs.clone(),
            StdArc::new(RunRegistry::new(self.fixture.path())),
            options,
        )
    }

    pub fn project_folder(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.project.path)
    }

    /// Adds a Service with a unique Key.
    pub fn service(&self, command_line: &str) -> Service {
        self.add(NewService::new("Service", unique_key("svc"), command_line), vec![], vec![])
    }

    pub fn add(&self, service: NewService, variables: Vec<VariableInput>, dependencies: Vec<Id>) -> Service {
        self.library.add_service(self.project.id, service, variables, dependencies, Origin::MainWindow).unwrap()
    }
}

static KEY_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn unique_key(prefix: &str) -> Key {
    key(&format!("{prefix}-{}", KEY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))
}

/// Collects the events for one Service until `is_last` returns true.
pub async fn events_until(
    events: &mut UnboundedReceiver<ServiceEvent>,
    service_id: Id,
    is_last: impl Fn(&ServiceEvent) -> bool,
) -> Vec<ServiceEvent> {
    let mut collected = Vec::new();
    let collect = async {
        while let Some(event) = events.recv().await {
            if event.service_id() != service_id {
                continue;
            }
            let done = is_last(&event);
            collected.push(event);
            if done {
                return;
            }
        }
    };
    tokio::time::timeout(PATIENCE, collect).await.expect("the expected event did not come");
    collected
}

/// Collects events of every Service until `is_last` returns true.
pub async fn all_events_until(
    events: &mut UnboundedReceiver<ServiceEvent>,
    is_last: impl Fn(&ServiceEvent) -> bool,
) -> Vec<ServiceEvent> {
    let mut collected = Vec::new();
    let collect = async {
        while let Some(event) = events.recv().await {
            let done = is_last(&event);
            collected.push(event);
            if done {
                return;
            }
        }
    };
    tokio::time::timeout(PATIENCE, collect).await.expect("the expected event did not come");
    collected
}

pub fn reached(state: ServiceState, service_id: Id) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::ServiceStateChanged(s) if s.service_id == service_id && s.state == state)
}

pub fn run_started(service_id: Id) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::RunStarted(run) if run.service_id == service_id)
}

pub fn run_ended(service_id: Id) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::RunEnded(run) if run.service_id == service_id)
}

pub fn is_state(state: ServiceState) -> impl Fn(&ServiceEvent) -> bool {
    move |event| matches!(event, ServiceEvent::ServiceStateChanged(s) if s.state == state)
}

pub fn is_run_ended(event: &ServiceEvent) -> bool {
    matches!(event, ServiceEvent::RunEnded(_))
}

pub fn states(events: &[ServiceEvent]) -> Vec<ServiceState> {
    events
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::ServiceStateChanged(status) => Some(status.state),
            _ => None,
        })
        .collect()
}

/// The statuses of one Service until `is_last` returns true.
pub async fn statuses_until(
    events: &mut UnboundedReceiver<ServiceEvent>,
    service_id: Id,
    is_last: impl Fn(&ServiceStatus) -> bool,
) -> Vec<ServiceStatus> {
    let seen =
        events_until(events, service_id, |event| matches!(event, ServiceEvent::ServiceStateChanged(s) if is_last(s)))
            .await;
    seen.into_iter()
        .filter_map(|event| match event {
            ServiceEvent::ServiceStateChanged(status) => Some(status),
            _ => None,
        })
        .collect()
}

/// Polls until `condition` holds.
pub async fn wait_until(mut condition: impl FnMut() -> bool) {
    let wait = async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(PATIENCE, wait).await.expect("the condition was not met");
}

/// A port nothing listens on right now (bound once by the kernel's choice,
/// then released), at least 100 away from any port another test of this
/// binary was given: tests run in parallel, the kernel picks nearby ports,
/// and a Service whose Port is taken moves up to 100 ports above it.
pub fn free_port() -> u16 {
    static HANDED_OUT: Mutex<Option<HashSet<u16>>> = Mutex::new(None);
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut handed_out = HANDED_OUT.lock().unwrap();
        let ports = handed_out.get_or_insert_with(HashSet::new);
        if ports.iter().all(|other| other.abs_diff(port) > 100) {
            ports.insert(port);
            return port;
        }
    }
}

/// A command line whose shell leaves behind a subshell that ignores SIGINT
/// and SIGTERM. The parent runs `then_parent` only once the subshell's trap
/// is in place, so the fixture can't race the signals.
pub fn leftover_ignoring_signals(then_parent: &str) -> String {
    let marker = std::env::temp_dir().join(format!("localhost-ready-{}", Id::new()));
    format!(
        "(trap '' INT TERM; touch '{marker}'; sleep 1000) & \
         while [ ! -e '{marker}' ]; do sleep 0.01; done; rm '{marker}'; {then_parent}",
        marker = marker.display()
    )
}

#[cfg(unix)]
pub fn process_group_exists(pgid: i32) -> bool {
    unsafe { libc::kill(-pgid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}
