//! What the app of one Machine and `localhost remote --stdio` on another
//! say to each other: JSON, one message per line, as the shared core's
//! protocol, but versioned on its own. The two ends are two computers, so
//! they often run different versions of localhost. This protocol only ever
//! grows: a field added later has a default, a request one end doesn't know
//! is answered with `Unsupported`, and a message it doesn't know is skipped.
//! `MACHINE_PROTOCOL_VERSION` says what the other end may ask for.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{Id, LocalhostError, OutputLine, Project, ServiceEvent, ServiceOutputLine};

/// Grows by one whenever a request or message is added. Never reset.
pub const MACHINE_PROTOCOL_VERSION: u32 = 1;

/// The first line `localhost remote --stdio` writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineHello {
    pub machine_protocol: u32,
    /// The version of localhost it was built from.
    pub version: String,
    /// The Machine's own name for itself, e.g. its host name.
    #[serde(default)]
    pub host_name: String,
}

impl MachineHello {
    pub fn current() -> MachineHello {
        MachineHello {
            machine_protocol: MACHINE_PROTOCOL_VERSION,
            version: crate::VERSION.to_string(),
            host_name: host_name(),
        }
    }
}

#[cfg(unix)]
fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| {
            let output = std::process::Command::new("hostname").output().ok()?;
            String::from_utf8(output.stdout).ok()
        })
        .map(|name| name.trim().to_string())
        .unwrap_or_default()
}

#[cfg(windows)]
fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MachineCall {
    pub id: u64,
    #[serde(flatten)]
    pub request: MachineRequest,
}

/// What the viewing app asks. Every change is made as `Origin::Machine`:
/// the other end decides the Origin, not the app.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum MachineRequest {
    /// Replies with nothing: the connection is alive.
    Ping,
    /// Replies with the Projects; then `Projects` follows every change.
    Projects,
    /// Replies with every status; then every Service event follows.
    Subscribe,
    Start {
        service_id: Id,
        on_port: Option<u16>,
    },
    Stop {
        service_id: Id,
    },
    Restart {
        service_id: Id,
    },
    StartAll {
        project_id: Id,
    },
    StopAll {
        project_id: Id,
    },
    /// Replies with the Run's buffered lines; the rest arrive as `RunLog`.
    OpenRunLog {
        stream: u64,
        run_id: Id,
    },
    /// Replies with the buffered lines; the rest arrive as `ServicesLog`.
    OpenServicesLog {
        stream: u64,
        service_ids: HashSet<Id>,
    },
    CloseLog {
        stream: u64,
    },
    /// A request from a newer app.
    #[serde(other)]
    Unsupported,
}

/// What `localhost remote --stdio` writes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum MachineMessage {
    Hello(MachineHello),
    /// Instead of the hello: it can't serve, in its own words. An app that
    /// doesn't know it sees the connection end.
    Refused {
        reason: String,
    },
    Reply {
        id: u64,
        result: Result<serde_json::Value, LocalhostError>,
    },
    /// Every Project, after a change.
    Projects {
        projects: Vec<Project>,
    },
    Event {
        event: ServiceEvent,
    },
    RunLog {
        stream: u64,
        lines: Vec<OutputLine>,
        is_finished: bool,
    },
    ServicesLog {
        stream: u64,
        lines: Vec<ServiceOutputLine>,
    },
    /// A message from a newer Machine.
    #[serde(other)]
    Unknown,
}
