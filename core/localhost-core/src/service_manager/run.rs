use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::process::Termination;
use crate::{Id, Listener, LocalhostError, Origin, Service};

/// The single, unambiguous status of a Service (see `GLOSSARY.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum ServiceState {
    /// Never started, or deliberately stopped, whatever exit code the stop produced.
    Stopped,
    /// A Run has been launched but the Service is not yet considered up.
    Starting,
    /// A Run is alive and has passed Starting.
    Running,
    /// A stop was requested and the Run's process tree has not yet fully ended.
    Stopping,
    /// The Run ended on its own with exit code 0, without anyone asking it to stop.
    Exited,
    /// The Run ended on its own unsuccessfully, or could not be launched at all.
    Failed,
}

impl ServiceState {
    /// Whether a Run is active (its process tree may be alive).
    pub fn is_active(&self) -> bool {
        matches!(self, ServiceState::Starting | ServiceState::Running | ServiceState::Stopping)
    }

    /// The state that says most about a group of Services, e.g. a Project's
    /// in the sidebar: Failed, then Stopping, Starting, Running, Exited,
    /// Stopped. `None` for no states.
    pub fn most_significant(states: impl IntoIterator<Item = ServiceState>) -> Option<ServiceState> {
        use ServiceState::*;
        let rank = |state: &ServiceState| {
            [Failed, Stopping, Starting, Running, Exited, Stopped].iter().position(|s| s == state)
        };
        states.into_iter().min_by_key(rank)
    }
}

/// One execution of a Service, from launch until its process tree has fully ended.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Run {
    pub id: Id,
    pub service_id: Id,
    /// Who started the Run.
    pub origin: Origin,
    pub started_at: SystemTime,
    /// When the process tree fully ended; `None` while it is still alive.
    pub ended_at: Option<SystemTime>,
    /// How the command's shell ended; `None` until it has, or if it never launched.
    pub termination: Option<Termination>,
    /// Why the Run could not launch or run its command (e.g. `CommandNotFound`).
    pub failure: Option<LocalhostError>,
    /// Who asked the Run to stop, if anyone did.
    pub stop_origin: Option<Origin>,
    /// Whether the process group had to be killed with SIGKILL.
    pub was_force_killed: bool,
    /// The TCP port the Run was given as `PORT`, if its Service has a Port.
    pub port: Option<u16>,
    /// Why `port` is not the Service's Port, if it isn't.
    pub port_change: Option<PortChange>,
    /// Whether the Run became Running because its Port did not accept
    /// connections in time; cleared once it does.
    pub is_port_unresponsive: bool,
    /// The first local URL the Run printed (`0.0.0.0` shown as `localhost`).
    pub detected_url: Option<String>,
}

impl Run {
    pub(crate) fn new(service_id: Id, origin: Origin) -> Run {
        Run {
            id: Id::new(),
            service_id,
            origin,
            started_at: SystemTime::now(),
            ended_at: None,
            termination: None,
            failure: None,
            stop_origin: None,
            was_force_killed: false,
            port: None,
            port_change: None,
            is_port_unresponsive: false,
            detected_url: None,
        }
    }
}

/// A Run started on another port than its Service's Port. The Project
/// Configuration keeps the Port; the change lasts for that Run (ADR-0006).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct PortChange {
    pub configured_port: u16,
    pub reason: PortChangeReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum PortChangeReason {
    /// Moved automatically because the Port was taken, by `holder` if
    /// localhost could see who.
    PortInUse { holder: Option<Box<Listener>> },
    /// Someone asked to start on this port.
    Requested,
}

/// A Service's state together with its active Run, or its most recent one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ServiceStatus {
    pub service_id: Id,
    pub state: ServiceState,
    pub run: Option<Run>,
    /// While Starting: the Dependencies the Service is still waiting for
    /// before its Run is launched. `run` is then the previous Run, if any.
    pub awaited_dependencies: Vec<Id>,
    /// While the Run is Starting or Running: its launch configuration
    /// (command line, working directory, variables, Port) differs from what
    /// a start would use now. Nothing restarts on its own.
    pub is_restart_needed: bool,
}

impl ServiceStatus {
    pub fn never_run(service_id: Id) -> ServiceStatus {
        ServiceStatus {
            service_id,
            state: ServiceState::Stopped,
            run: None,
            awaited_dependencies: Vec::new(),
            is_restart_needed: false,
        }
    }
}

/// A change in a Service's lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum ServiceEvent {
    ServiceStateChanged(ServiceStatus),
    RunStarted(Run),
    RunEnded(Run),
}

impl ServiceEvent {
    pub fn service_id(&self) -> Id {
        match self {
            ServiceEvent::ServiceStateChanged(status) => status.service_id,
            ServiceEvent::RunStarted(run) | ServiceEvent::RunEnded(run) => run.service_id,
        }
    }
}

impl Service {
    /// The URL to open for this Service while `run` is its latest Run: the
    /// configured URL with its port moved to the Run's, else `localhost` at
    /// the Run's port, else the first local URL the Run printed.
    pub fn url_to_open(&self, run: Option<&Run>) -> Option<String> {
        let actual_port = run.and_then(|run| run.port).or(self.port);
        if let Some(url) = &self.url {
            return Some(match (self.port, actual_port) {
                (Some(port), Some(actual)) => with_port_moved(url, port, actual),
                _ => url.clone(),
            });
        }
        if let Some(port) = actual_port {
            return Some(format!("http://localhost:{port}"));
        }
        run.and_then(|run| run.detected_url.clone())
    }
}

/// `url` with its explicit port changed from `from` to `to`, keeping the rest as written.
fn with_port_moved(url: &str, from: u16, to: u16) -> String {
    let Some(scheme_end) = url.find("://").map(|index| index + 3) else { return url.to_string() };
    let authority_end = url[scheme_end..].find(['/', '?', '#']).map_or(url.len(), |index| scheme_end + index);
    match url[scheme_end..authority_end].strip_suffix(&format!(":{from}")) {
        Some(host) => format!("{}{host}:{to}{}", &url[..scheme_end], &url[authority_end..]),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Key;

    fn service(port: Option<u16>, url: Option<&str>) -> Service {
        Service {
            id: Id::new(),
            name: "Web".into(),
            key: Key::new("web").unwrap(),
            working_directory: ".".into(),
            command_line: "x".into(),
            environment: Default::default(),
            secret_names: Vec::new(),
            dependencies: Vec::new(),
            is_excluded_from_start_all: false,
            port,
            url: url.map(String::from),
            port_can_change: true,
        }
    }

    #[test]
    fn opens_the_configured_url_moved_to_the_runs_port_before_localhost_at_the_port() {
        let mut run = Run::new(Id::new(), Origin::MainWindow);
        run.port = Some(3001);
        run.detected_url = Some("http://localhost:9999".into());

        let configured = service(Some(3000), Some("http://localhost:3000/admin"));
        assert_eq!(configured.url_to_open(Some(&run)).as_deref(), Some("http://localhost:3001/admin"));

        let other_host = service(Some(3000), Some("https://app.test"));
        assert_eq!(other_host.url_to_open(Some(&run)).as_deref(), Some("https://app.test"));

        let port_only = service(Some(3000), None);
        assert_eq!(port_only.url_to_open(Some(&run)).as_deref(), Some("http://localhost:3001"));
        assert_eq!(port_only.url_to_open(None).as_deref(), Some("http://localhost:3000"));

        let neither = service(None, None);
        assert_eq!(neither.url_to_open(Some(&run)).as_deref(), Some("http://localhost:3001"));
        run.port = None;
        assert_eq!(neither.url_to_open(Some(&run)).as_deref(), Some("http://localhost:9999"));
        assert_eq!(neither.url_to_open(None), None);
    }

    #[test]
    fn moves_only_the_explicit_port() {
        assert_eq!(with_port_moved("http://localhost:3000", 3000, 3001), "http://localhost:3001");
        assert_eq!(with_port_moved("http://[::1]:3000/x?y#z", 3000, 4000), "http://[::1]:4000/x?y#z");
        assert_eq!(with_port_moved("http://localhost:30000/", 3000, 3001), "http://localhost:30000/");
        assert_eq!(with_port_moved("http://localhost/a:3000", 3000, 3001), "http://localhost/a:3000");
    }

    #[test]
    fn the_most_significant_state_puts_failures_first() {
        use ServiceState::*;
        assert_eq!(ServiceState::most_significant([Running, Failed, Stopped]), Some(Failed));
        assert_eq!(ServiceState::most_significant([Stopped, Running, Starting]), Some(Starting));
        assert_eq!(ServiceState::most_significant([Exited, Stopped]), Some(Exited));
        assert_eq!(ServiceState::most_significant([]), None);
    }
}
