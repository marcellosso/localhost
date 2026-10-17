//! Driving a localhost core that runs somewhere else, over a byte stream:
//! the Windows app drives `localhostd --stdio` inside a WSL distro, where
//! the distro's Services, Listeners and Orphans are (ADR-0008).
//!
//! The protocol is JSON, one message per line. `localhostd` opens with a
//! [`Hello`]. Then the app sends requests, each with an id, and `localhostd`
//! answers each with a reply carrying that id, in any order. Service events
//! and log batches arrive between replies.
//!
//! The app keeps the Project Configuration: it sends the Projects, with the
//! values of their Secrets, and `localhostd` holds them in memory only.
//! When its input closes (the app quit or crashed), `localhostd` stops
//! every Run, as the in-process core does when the app quits.

mod client;
#[cfg(unix)]
mod server;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use client::Remote;
#[cfg(unix)]
pub use server::serve;

use crate::{
    Id, Listener, LocalhostError, Origin, Orphan, OutputLine, Project, RunStats, ServiceEvent, ServiceOutputLine,
};

/// Changes whenever a message changes shape; both ends must speak the same.
pub const PROTOCOL_VERSION: u32 = 2;

/// The first line `localhostd` writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol: u32,
    /// The version of localhost it was built from.
    pub version: String,
}

impl Hello {
    pub fn current() -> Hello {
        Hello { protocol: PROTOCOL_VERSION, version: env!("CARGO_PKG_VERSION").to_string() }
    }
}

/// A request from the app, and the id its reply carries.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Call {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum Request {
    /// Replaces the Projects, and the Secret values by account.
    SetProjects {
        projects: Vec<Project>,
        secrets: HashMap<String, String>,
    },
    /// Replies with every status; then every Service event follows.
    Subscribe,
    Start {
        service_id: Id,
        on_port: Option<u16>,
        origin: Origin,
    },
    Stop {
        service_id: Id,
        origin: Origin,
    },
    Restart {
        service_id: Id,
        origin: Origin,
    },
    StartAll {
        project_id: Id,
        origin: Origin,
    },
    StopAll {
        project_id: Id,
        origin: Origin,
    },
    StopEveryRun {
        origin: Origin,
    },
    SetAutomaticPortMove {
        is_enabled: bool,
    },
    Listeners,
    StopListener {
        listener: Listener,
        origin: Origin,
    },
    RunStats {
        service_id: Id,
        previous: Option<RunStats>,
    },
    NextFreePort {
        service_id: Id,
    },
    Orphans,
    StopOrphans {
        orphans: Vec<Orphan>,
    },
    LeaveOrphans {
        orphans: Vec<Orphan>,
    },
    RemoveLogs {
        service_id: Id,
    },
    ClearBuffer {
        service_id: Id,
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
    ScanFolder {
        path: String,
    },
    ScanProject {
        project: Project,
    },
}

/// What `localhostd` writes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum Message {
    Hello(Hello),
    Reply { id: u64, result: Result<serde_json::Value, LocalhostError> },
    Event { event: ServiceEvent },
    RunLog { stream: u64, lines: Vec<OutputLine>, is_finished: bool },
    ServicesLog { stream: u64, lines: Vec<ServiceOutputLine> },
}
