//! What the app of one Machine and `localhost remote --stdio` on another
//! say to each other: JSON, one message per line, as the shared core's
//! protocol, but versioned on its own. The two ends are two computers, so
//! they often run different versions of localhost. This protocol only ever
//! grows: a field added later has a default, a request one end doesn't know
//! is answered with `Unsupported`, and a message it doesn't know is skipped.
//! `MACHINE_PROTOCOL_VERSION` says what the other end may ask for.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    Id, KeyClash, LocalhostError, NewService, OutputLine, ProfileInput, Project, ProjectChanges, ServiceChanges,
    ServiceEvent, ServiceOutputLine, VariableInput, VariableLayer,
};

/// Grows by one whenever a request or message is added. Never reset.
///
/// 2: the Project Configuration is changed from the app (`MANAGES_PROJECTS`).
pub const MACHINE_PROTOCOL_VERSION: u32 = 2;

/// The first version whose Projects the app can add, edit and remove.
pub const MANAGES_PROJECTS: u32 = 2;

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

    // Since `MANAGES_PROJECTS`: changes to the Project Configuration there,
    // validated and saved there, as its own app makes them.
    /// Replies with a `DirectoryListing`; `None` is the home folder.
    ListDirectory {
        path: Option<String>,
        show_hidden: bool,
        include_files: bool,
    },
    /// Replies with the Suggestions for a folder there, once it exists.
    ScanFolder {
        path: String,
    },
    ScanProject {
        project_id: Id,
    },
    DetectPythonEnvironment {
        project_id: Id,
        working_directory: String,
        command_line: String,
    },
    AddProject {
        name: String,
        key: crate::Key,
        path: String,
        services: Vec<NewService>,
    },
    EditProject {
        project_id: Id,
        changes: ProjectChanges,
    },
    RemoveProject {
        project_id: Id,
    },
    AddService {
        project_id: Id,
        service: NewService,
        variables: Vec<VariableInput>,
        dependencies: Vec<Id>,
    },
    AddServices {
        project_id: Id,
        services: Vec<NewService>,
    },
    EditService {
        service_id: Id,
        changes: ServiceChanges,
    },
    RemoveService {
        service_id: Id,
    },
    SaveEnvironment {
        project_id: Id,
        variables: Vec<VariableInput>,
        profiles: Vec<ProfileInput>,
        active: Option<u32>,
    },
    SetActiveProfile {
        project_id: Id,
        profile_id: Option<Id>,
    },
    /// Replies with a stored Secret's value, to reveal it in the editor.
    SecretValue {
        name: String,
        layer: VariableLayer,
    },
    /// Replies with a `ProjectFileText`.
    ReadProjectFile {
        path: String,
    },
    /// Imports the Project file's `text` into the folder `path` there.
    ImportProject {
        text: String,
        path: String,
        on_key_clash: Option<KeyClash>,
    },
    /// A request from a newer app.
    #[serde(other)]
    Unsupported,
}

/// A Project file on the Machine, for the import's review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ProjectFileText {
    pub text: String,
    /// Whether the folder the file names is there.
    pub folder_exists: bool,
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
