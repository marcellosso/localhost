//! localhost-core: the UI-free core of localhost.
//!
//! Every client (the macOS app, the Windows/Linux app, the CLI) is a client of
//! this crate. It never depends on a UI toolkit. State-changing calls take an
//! [`Origin`], failures are a [`LocalhostError`], and changes are broadcast as
//! events, so the core can later be hosted behind IPC without a rewrite.

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!("localhost_core");

mod ansi;
mod dependencies;
mod detection;
mod error;
#[cfg(feature = "ffi")]
mod ffi;
mod fold;
mod fuzzy;
mod id;
mod key;
mod library;
mod listener;
mod local_url;
mod logs;
mod model;
mod origin;
mod output;
mod paths;
mod platform;
mod port_probe;
mod process;
mod project_document;
pub mod remote;
mod run_environment;
mod run_registry;
mod run_stats;
mod secrets;
mod service_manager;
mod storage;
mod targets;
mod validate;
mod yaml;

pub use ansi::{Ansi, AnsiColor, Span, StyledLine, TextStyle};
pub use detection::{
    CargoDetector, CommandSyntax, ComposeDetector, NodeDetector, ProjectScanner, PythonDetector, ScannedDirectory,
    ServiceDetector, ServiceSuggestion,
};
pub use error::LocalhostError;
pub use fuzzy::FuzzyMatch;
pub use id::Id;
pub use key::Key;
pub use library::{KeyClash, ProfileChanges, ProjectChanges, ProjectLibrary, ProjectLibraryEvent, ServiceChanges};
pub use listener::{Listener, ListenerKind, ListenerOwner, PortWarning, port_warnings};
pub use local_url::LocalUrl;
pub use logs::{LogBuffer, LogStore};
pub use model::{
    EnvironmentProfile, ExecutionTarget, NewService, Project, Service, VariableInput, VariableLayer, VariableValue,
    Variables, service_label, suggested_project_key,
};
pub use origin::Origin;
pub use output::{OutputChannel, OutputLine, ServiceOutputLine};
pub use paths::default_storage_root;
pub use platform::ProcessStartTime;
pub use process::{
    LaunchConfiguration, MAX_LINE_LENGTH, ProcessTree, ProcessTreeExit, StopPolicy, Termination, signal_name,
};
pub use project_document::{DocumentProfile, DocumentService, ProjectDocument, ProjectFileProblem, ValueKind};
pub use run_environment::RunEnvironment;
pub use run_registry::{Orphan, RunRegistry};
pub use run_stats::RunStats;
pub use secrets::{MemorySecretStore, SecretStore, SystemSecretStore};
pub use service_manager::{
    PortChange, PortChangeReason, Run, ServiceEvent, ServiceManager, ServiceManagerOptions, ServiceState, ServiceStatus,
};
pub use storage::UnreadableProjectFile;
pub use targets::{Connecting, FoundOrphans, Launcher, Link, Targets};

/// How serde writes a `SystemTime`, for the TypeScript types: specta's own
/// description of it doesn't match.
#[cfg(feature = "specta")]
#[derive(specta::Type)]
#[specta(rename = "SystemTime")]
#[allow(dead_code)]
pub(crate) struct SerdeSystemTime {
    secs_since_epoch: u64,
    nanos_since_epoch: u32,
}

/// Version of the core, surfaced by clients.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
