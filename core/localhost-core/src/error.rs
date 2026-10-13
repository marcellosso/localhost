use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Id, Key, Listener, ProjectFileProblem};

/// Every failure the core reports. Clients match on these cases; they are
/// never flattened into strings inside the core.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum LocalhostError {
    // Validation
    InvalidKey(String),
    InvalidName,
    InvalidProjectPath(String),
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
            PortConflict { port, owner: Some(owner) } => {
                format!("Port {port} is in use by {} (PID {}).", owner.name, owner.pid)
            }
            PortConflict { port, owner: None } => format!("Port {port} is in use by another process."),
            ListenerIsLocalhost => "localhost can't stop itself.".into(),
            ListenerIsLaunchdJob { label } => format!(
                "It is managed by launchd ({label}), which would start it again. Stop it with the tool that started it."
            ),
            ListenerIsDocker => "This is Docker listening for a container. Stop the container instead.".into(),
            SecretStoreFailed(reason) => format!("{} refused: {reason}.", crate::secrets::STORE_NAME),
            UnsupportedSchemaVersion { version, path } => {
                format!("{path} was saved by a newer version of localhost (schema {version}).")
            }
            PersistenceFailed { path, reason } => format!("Could not read or write {path}: {reason}"),
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
fn describe_errno(errno: i32) -> String {
    let description = std::io::Error::from_raw_os_error(errno).to_string();
    // std appends " (os error N)".
    match description.rfind(" (os error ") {
        Some(index) => description[..index].to_string(),
        None => description,
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
