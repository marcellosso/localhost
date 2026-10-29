//! Driving a localhost core that runs somewhere else, over a byte stream:
//! the Windows app drives `localhostd --stdio` inside a WSL distro, where
//! the distro's Services, Listeners and Orphans are; every client drives
//! one shared core on this machine through a local socket, or a named pipe
//! on Windows ([`shared`]).
//!
//! The protocol is JSON, one message per line. `localhostd` opens with a
//! [`Hello`]. Then the app sends requests, each with an id, and `localhostd`
//! answers each with a reply carrying that id, in any order. Service events
//! and log batches arrive between replies.
//!
//! The client keeps the Project Configuration: it sends the Projects, with
//! the values of the Secrets a start needs, and the core holds them in
//! memory only, the latest a client sent winning. A value stays until a
//! client sends another, or no Project names its Secret.
//!
//! Runs belong to the core, not to a connection. A client says what it is
//! ([`Client`]); when one disconnects, the Runs nobody needs any more stop:
//! those it started, unless an app is still connected, or another
//! connected client started them too. So `localhostd --stdio`, whose only
//! client is the app, stops every Run when its input closes, as the
//! in-process core does when the app quits.

mod client;
mod server;
pub mod shared;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub use client::Remote;
pub use server::serve;

use crate::{
    Id, Listener, LocalhostError, Origin, Orphan, OutputLine, Project, RunStats, ServiceEvent, ServiceOutputLine,
};

/// Changes whenever a message changes shape; both ends must speak the same.
pub const PROTOCOL_VERSION: u32 = 4;

/// What a connection is, which decides what its disconnecting stops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Client {
    /// The macOS, Linux or Windows app. While one is connected, nothing
    /// stops because another client disconnected: the app shows it.
    #[default]
    App,
    /// A coding agent, through `localhost mcp`.
    Agent,
    /// The `localhost` command.
    Cli,
}

impl Client {
    /// The Origin of what the core does on this client's behalf, such as
    /// stopping its Runs as it disconnects.
    pub fn origin(self) -> Origin {
        match self {
            Client::App => Origin::MainWindow,
            Client::Agent => Origin::Agent,
            Client::Cli => Origin::Cli,
        }
    }
}

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
    /// Says what this connection is; one that never says is an app.
    Identify {
        client: Client,
    },
    /// Replaces the Projects, and sets the given Secret values by account
    /// (`None` has none).
    SetProjects {
        projects: Vec<Project>,
        secrets: HashMap<String, Option<String>>,
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
