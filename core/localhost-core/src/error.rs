use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, Key, Listener, MachineProblem, ProjectFileProblem};

/// Every failure the core reports. Clients match on these cases; they are
/// never flattened into strings inside the core.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Error))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum LocalhostError {
    // Validation
    InvalidKey(String),
    InvalidName,
    InvalidProjectPath(String),
    /// No folder is at the Project's path on this machine.
    ProjectFolderMissing(String),
    InvalidDistroName(String),
    InvalidWorkingDirectory(String),
    EmptyCommandLine,
    InvalidEnvironmentVariableName(String),
    DuplicateVariableName(String),
    DuplicateProfileName(String),
    InvalidPort(i64),
    InvalidUrl(String),
    DuplicateProjectKey(Key),
    DuplicateServiceKey {
        key: Key,
        project_id: Id,
    },
    /// The Dependency would make a Service depend on itself, e.g. `[api, worker, api]`.
    DependencyCycle(Vec<Key>),

    // Lookup
    ProjectNotFound(Id),
    ServiceNotFound(Id),
    ProfileNotFound(Id),
    /// A Service being added starts after a Key that is neither in its
    /// Project nor added with it.
    DependencyNotFound(Key),

    // Launching a Run
    WorkingDirectoryMissing(String),
    ShellNotFound(String),
    LaunchFailed {
        errno: i32,
    },
    /// The shell exited with 127: the command was not found.
    CommandNotFound,
    /// The shell exited with 126: the command was found but could not be executed.
    CommandNotExecutable,
    /// A Dependency Failed, so the Service was not started.
    DependencyFailed(Key),
    /// A Secret of the Run's environment has no stored value.
    SecretMissing(String),
    /// The Run failed after printing `Cannot connect to the Docker daemon`.
    DockerDaemonUnavailable,
    /// The core on the Project's Execution Target (`localhostd` in a WSL
    /// distro) can't be reached, or the connection to it was lost.
    TargetUnavailable(String),
    /// The shared core on this machine (`remote::shared`) can't be started
    /// or reached, or the connection to it was lost.
    CoreUnavailable(String),
    /// This machine's shared core is another version's, which this one
    /// can't drive: kept alive by an older app or a coding agent's
    /// `localhost mcp`.
    IncompatibleCore {
        version: String,
    },

    // Ports and Listeners
    /// Something already listens on the port; `owner` is that Listener, if
    /// localhost can see it (processes of other users can't be inspected).
    PortConflict {
        port: u16,
        owner: Option<Box<Listener>>,
    },
    /// localhost does not stop itself.
    ListenerIsLocalhost,
    /// launchd would restart it (a Homebrew service, an app): stop it there instead.
    ListenerIsLaunchdJob {
        label: String,
    },
    /// Docker's backend holds the ports of every running container.
    ListenerIsDocker,

    // Persistence
    /// The system's secret store (the Keychain, the Secret Service) refused.
    SecretStoreFailed(String),
    UnsupportedSchemaVersion {
        version: i64,
        path: String,
    },
    PersistenceFailed {
        path: String,
        reason: String,
    },

    // Project files
    /// A Project file can't be imported: `problem`, at `field` (e.g.
    /// `["services", "web", "port"]`) on `line`, when known.
    InvalidProjectFile {
        problem: ProjectFileProblem,
        field: Vec<String>,
        line: Option<usize>,
    },

    // Coding agents
    /// The agent (by name) isn't installed on this machine.
    AgentNotInstalled(String),
    /// The agent's configuration file (`path`) can't be read, or sets the
    /// `localhost` server in a way localhost doesn't edit.
    AgentConfigUnsupported {
        path: String,
        reason: String,
    },
    /// The app has no `localhost` command at this path: a build without it.
    CommandNotBundled(String),
    /// The app runs from a temporary location (a disk image, App
    /// Translocation), where agents couldn't find its command later.
    CommandInTemporaryLocation(String),
    /// Another file is where the `localhost` link would go.
    CommandPathTaken(String),
    /// An agent's own command (`claude mcp …`) failed: what it printed.
    AgentCommandFailed {
        command: String,
        output: String,
    },

    // Machines
    /// Not something `ssh` can connect to: empty, with spaces, or like an option.
    InvalidMachineDestination(String),
    /// Another Machine is already reached at this destination.
    DuplicateMachine(String),
    MachineNotFound(Id),
    /// Another Machine (by name) can't be reached over SSH, or the
    /// connection to it was lost.
    MachineUnavailable {
        machine: String,
        problem: MachineProblem,
    },
    /// The Port can't be forwarded from another Machine: SSH's reason.
    ForwardFailed {
        port: u16,
        reason: String,
    },
    /// The other end runs an older localhost, which can't do this.
    Unsupported,
    /// The Machine (by name) runs a localhost too old to change its
    /// Projects from here; they can still be viewed and run.
    MachineNeedsUpdate {
        machine: String,
    },
    /// A folder exists but can't be listed: the system's reason.
    FolderUnreadable {
        path: String,
        reason: String,
    },
}

impl LocalhostError {
    /// A short, user-facing explanation.
    pub fn message(&self) -> String {
        use LocalhostError::*;
        match self {
            InvalidKey(raw) => {
                format!("“{raw}” is not a valid key. Use lowercase letters, digits and single hyphens, e.g. my-app.")
            }
            InvalidName => "Enter a name.".into(),
            InvalidProjectPath(_) => "The project folder must be an absolute path.".into(),
            ProjectFolderMissing(path) => format!("There is no folder at {path}. Use an existing project folder."),
            InvalidDistroName(name) => format!("“{name}” is not the name of a WSL distro."),
            InvalidWorkingDirectory(_) => "The working directory must be relative to the project folder.".into(),
            EmptyCommandLine => "Enter a command line.".into(),
            InvalidEnvironmentVariableName(name) => format!("“{name}” is not a valid environment variable name."),
            DuplicateVariableName(name) => format!("“{name}” is defined more than once."),
            DuplicateProfileName(name) => format!("Another profile in this project is already called “{name}”."),
            InvalidPort(port) => format!("{port} is not a valid port. Use a number from 1 to 65535."),
            InvalidUrl(raw) => format!("“{raw}” is not a valid URL. Use a full http:// or https:// address."),
            DuplicateProjectKey(key) => format!("Another project already uses the key “{key}”."),
            DuplicateServiceKey { key, .. } => format!("Another service in this project already uses the key “{key}”."),
            DependencyCycle(keys) => {
                let path: Vec<&str> = keys.iter().map(Key::as_str).collect();
                format!("{} would be a dependency cycle.", path.join(" → "))
            }
            ProjectNotFound(_) => "The project no longer exists.".into(),
            ServiceNotFound(_) => "The service no longer exists.".into(),
            ProfileNotFound(_) => "The environment profile no longer exists.".into(),
            DependencyNotFound(key) => {
                format!("It starts after “{key}”, but no service in this project has that key.")
            }
            WorkingDirectoryMissing(path) => format!("The working directory {path} does not exist."),
            ShellNotFound(path) => format!("The shell {path} was not found."),
            LaunchFailed { errno } => format!("The command could not be launched: {}.", describe_errno(*errno)),
            CommandNotFound => "Command not found.".into(),
            CommandNotExecutable => "The command is not executable.".into(),
            DependencyFailed(key) => format!("Not started: its dependency “{key}” failed."),
            SecretMissing(name) => format!("The secret “{name}” has no stored value. Set it again."),
            DockerDaemonUnavailable => {
                "Docker isn’t running. Start Docker Desktop (or your Docker engine), then start the service again."
                    .into()
            }
            TargetUnavailable(reason) => format!("localhost can’t reach the WSL distro: {reason}."),
            CoreUnavailable(reason) => {
                format!("localhost’s background service isn’t running: {reason}. Quit localhost and open it again.")
            }
            IncompatibleCore { version } => format!(
                "localhost {version} is still running in the background, perhaps for a coding agent, and can’t work with this version. Quit it and any agent using `localhost mcp`, then try again."
            ),
            PortConflict { port, owner: Some(owner) } => {
                format!("Port {port} is in use by {} (PID {}).", owner.name, owner.pid)
            }
            PortConflict { port, owner: None } => format!("Port {port} is in use by another process."),
            ListenerIsLocalhost => "localhost can't stop itself.".into(),
            ListenerIsLaunchdJob { label } if cfg!(windows) => format!(
                "It is managed by Windows ({label}), which would start it again. Stop it in Services or with the tool that started it."
            ),
            ListenerIsLaunchdJob { label } => format!(
                "It is managed by launchd ({label}), which would start it again. Stop it with the tool that started it."
            ),
            ListenerIsDocker => "This is Docker listening for a container. Stop the container instead.".into(),
            SecretStoreFailed(reason) => format!("{} refused: {reason}.", crate::secrets::STORE_NAME),
            UnsupportedSchemaVersion { version, path } => {
                format!("{path} was saved by a newer version of localhost (schema {version}).")
            }
            PersistenceFailed { path, reason } => format!("Could not read or write {path}: {reason}"),
            AgentNotInstalled(name) => format!("{name} isn’t installed on this computer."),
            AgentConfigUnsupported { path, reason } => {
                format!("localhost can’t edit {path}: {reason}. Fix the file, or set up the server by hand.")
            }
            CommandNotBundled(path) => format!(
                "This build of localhost has no localhost command ({path}). Install localhost from a release to connect agents."
            ),
            CommandInTemporaryLocation(_) => "localhost is running from a temporary location. Move it to Applications, open it from there, and try again.".into(),
            AgentCommandFailed { command, output } => {
                format!("`{command}` failed: {output}. Try again, or set up the server by hand.")
            }
            CommandPathTaken(path) => format!("{path} already exists and isn’t localhost’s. Remove it first, or leave it."),
            InvalidMachineDestination(raw) => format!(
                "“{raw}” isn’t something ssh can connect to. Use a Host from ~/.ssh/config, or user@host."
            ),
            DuplicateMachine(destination) => format!("{destination} is already one of your Machines."),
            MachineNotFound(_) => "The Machine no longer exists.".into(),
            MachineUnavailable { machine, problem } => problem.message(machine),
            ForwardFailed { port, reason } => format!("Port {port} couldn’t be forwarded: {reason}."),
            Unsupported => "The other Machine runs an older localhost that can’t do this. Update localhost there.".into(),
            MachineNeedsUpdate { machine } => format!("Update localhost on {machine} to manage its Projects."),
            FolderUnreadable { path, reason } => format!("{path} can’t be opened: {reason}."),
            InvalidProjectFile { problem, field, line } => {
                let line = line.map(|line| format!("Line {line}"));
                let field = (!field.is_empty()).then(|| field.join(" › "));
                let place: Vec<String> = [line, field].into_iter().flatten().collect();
                if place.is_empty() {
                    problem.message()
                } else {
                    format!("{}: {}", place.join(", "), problem.message())
                }
            }
        }
    }
}

/// The system's description of an errno value, e.g. "Cannot allocate memory".
#[cfg(unix)]
fn describe_errno(errno: i32) -> String {
    let description = std::io::Error::from_raw_os_error(errno).to_string();
    // std appends " (os error N)".
    match description.rfind(" (os error ") {
        Some(index) => description[..index].to_string(),
        None => description,
    }
}

/// The description of a Linux errno value: on Windows, a launch fails in a
/// WSL distro, so its errno is Linux's, not a Windows error code.
#[cfg(not(unix))]
fn describe_errno(errno: i32) -> String {
    match errno {
        1 => "Operation not permitted".into(),
        2 => "No such file or directory".into(),
        7 => "Argument list too long".into(),
        8 => "Exec format error".into(),
        11 => "Resource temporarily unavailable".into(),
        12 => "Cannot allocate memory".into(),
        13 => "Permission denied".into(),
        20 => "Not a directory".into(),
        22 => "Invalid argument".into(),
        24 => "Too many open files".into(),
        other => format!("error {other}"),
    }
}

impl fmt::Display for LocalhostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for LocalhostError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_what_went_wrong() {
        assert_eq!(
            LocalhostError::DuplicateProjectKey(Key::new("api").unwrap()).message(),
            "Another project already uses the key “api”."
        );
        assert_eq!(
            LocalhostError::WorkingDirectoryMissing("/tmp/x".into()).message(),
            "The working directory /tmp/x does not exist."
        );
        assert_eq!(LocalhostError::CommandNotFound.message(), "Command not found.");
        // ENOMEM is 12 on macOS and Linux.
        assert!(LocalhostError::LaunchFailed { errno: 12 }.message().contains("Cannot allocate memory"));
        assert!(!LocalhostError::LaunchFailed { errno: 12 }.message().contains("os error"));
    }

    #[test]
    fn says_where_the_problem_in_a_project_file_is() {
        let error = LocalhostError::InvalidProjectFile {
            problem: ProjectFileProblem::Invalid(Box::new(LocalhostError::InvalidPort(70000))),
            field: vec!["services".into(), "web".into(), "port".into()],
            line: Some(8),
        };
        assert_eq!(
            error.message(),
            "Line 8, services › web › port: 70000 is not a valid port. Use a number from 1 to 65535."
        );
        let error = LocalhostError::InvalidProjectFile {
            problem: ProjectFileProblem::NotAProjectFile,
            field: vec![],
            line: None,
        };
        assert_eq!(error.message(), "This isn’t a localhost project file: it has no “localhost: 1” line.");
    }
}
