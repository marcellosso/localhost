//! localhost-core: the UI-free core of localhost (ADR-0001, ADR-0007).
//!
//! Every client (the macOS app, the Windows/Linux app, the CLI) is a client of
//! this crate. It never depends on a UI toolkit. State-changing calls take an
//! [`Origin`], failures are a [`LocalhostError`], and changes are broadcast as
//! events, so the core can later be hosted behind IPC without a rewrite.

mod ansi;
mod dependencies;
mod error;
mod fold;
mod fuzzy;
mod id;
mod key;
mod library;
#[allow(dead_code)] // Scanning and stopping are used by the ServiceManager, ported next.
mod listener;
mod local_url;
mod logs;
mod model;
mod origin;
mod output;
mod paths;
#[allow(dead_code)] // Scanning is used by the ServiceManager, ported next.
mod platform;
#[allow(dead_code)] // Used by the ServiceManager, ported next.
mod port_probe;
mod process;
mod run_environment;
#[allow(dead_code)] // Recording is used by the ServiceManager, ported next.
mod run_registry;
mod secrets;
mod storage;
mod validate;

pub use ansi::{Ansi, AnsiColor, Span, StyledLine, TextStyle};
pub use error::LocalhostError;
pub use fuzzy::FuzzyMatch;
pub use id::Id;
pub use key::Key;
pub use library::{KeyClash, ProfileChanges, ProjectChanges, ProjectLibrary, ProjectLibraryEvent, ServiceChanges};
pub use listener::{Listener, ListenerKind, ListenerOwner};
pub use local_url::LocalUrl;
pub use logs::{LogBuffer, LogStore};
pub use model::{
    EnvironmentProfile, NewService, Project, Service, VariableInput, VariableLayer, VariableValue, Variables,
};
pub use origin::Origin;
pub use output::{OutputChannel, OutputLine, ServiceOutputLine};
pub use paths::default_storage_root;
pub use platform::ProcessStartTime;
pub use process::{
    LaunchConfiguration, MAX_LINE_LENGTH, ProcessTree, ProcessTreeExit, StopPolicy, Termination, signal_name,
};
pub use run_environment::RunEnvironment;
pub use run_registry::{Orphan, RunRegistry};
pub use secrets::{MemorySecretStore, SecretStore, SystemSecretStore};
pub use storage::UnreadableProjectFile;

/// Version of the core, surfaced by clients (and later the CLI/MCP).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
