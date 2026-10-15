//! The Project Configuration: Projects, Services, variables and Environment
//! Profiles. Serialized exactly as the Swift core stored them (schema 1).
//!
//! Fields are public to read. Values are created and changed only through
//! [`ProjectLibrary`](crate::ProjectLibrary), which validates them, so every
//! Project a client gets from the library is valid.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::standardize;
use crate::{Id, Key};

/// One local development project, rooted at a folder on the developer's
/// machine, grouping the Services needed to work on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub key: Key,
    /// Absolute path of the Project's root folder, on its Execution Target.
    pub path: String,
    /// Where its Services run. Left out for this machine, so files of
    /// Projects on this machine are unchanged since before Execution Targets.
    // `default` last: specta reads it as optional, and the other attribute as not.
    #[serde(skip_serializing_if = "ExecutionTarget::is_this_machine", default)]
    pub execution_target: ExecutionTarget,
    pub sort_order: i64,
    pub is_favorite: bool,
    pub services: Vec<Service>,
    /// Variables every Service of the Project gets, below its own.
    #[serde(default)]
    pub variables: Variables,
    #[serde(default)]
    pub profiles: Vec<EnvironmentProfile>,
    /// The Environment Profile applied on top of every Service's variables, if any.
    #[serde(rename = "activeProfileID", default, skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<Id>,
}

impl Project {
    pub(crate) fn new(name: String, key: Key, path: String, sort_order: i64) -> Project {
        Project {
            id: Id::new(),
            name,
            key,
            path,
            execution_target: ExecutionTarget::ThisMachine,
            sort_order,
            is_favorite: false,
            services: Vec::new(),
            variables: Variables::default(),
            profiles: Vec::new(),
            active_profile_id: None,
        }
    }

    pub fn active_profile(&self) -> Option<&EnvironmentProfile> {
        self.active_profile_id.and_then(|id| self.profile(id))
    }

    pub fn profile(&self, id: Id) -> Option<&EnvironmentProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn service(&self, id: Id) -> Option<&Service> {
        self.services.iter().find(|service| service.id == id)
    }

    /// The absolute working directory a Service runs in.
    pub fn working_directory(&self, service: &Service) -> PathBuf {
        standardize(&PathBuf::from(&self.path).join(&service.working_directory))
    }

    /// A Key for a Service named `name` that no other Service of the Project
    /// uses (`excluding` is the Service being edited, if any).
    pub fn suggested_service_key(&self, name: &str, excluding: Option<Id>) -> Key {
        Key::suggested_unique(name, "service", |key| {
            self.services.iter().any(|service| Some(service.id) != excluding && service.key.as_str() == key)
        })
    }
}

#[cfg(test)]
impl Project {
    /// A Project named `name` with a Service per `(name, port)`; Keys are
    /// suggested from the names.
    pub(crate) fn with_services(name: &str, services: &[(&str, Option<u16>)]) -> Project {
        let mut project = Project::new(name.into(), Key::suggested_from(name).unwrap(), "/tmp".into(), 0);
        project.services = services
            .iter()
            .map(|&(name, port)| Service {
                id: Id::new(),
                name: name.into(),
                key: Key::suggested_from(name).unwrap(),
                working_directory: ".".into(),
                command_line: "x".into(),
                environment: BTreeMap::new(),
                secret_names: Vec::new(),
                dependencies: Vec::new(),
                is_excluded_from_start_all: false,
                port,
                url: None,
                port_can_change: true,
            })
            .collect();
        project
    }
}

/// Where a Project's Services run: this machine, or a WSL distro, where
/// `localhostd` runs them (ADR-0008).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ExecutionTarget {
    #[default]
    ThisMachine,
    /// A WSL distro, by the name `wsl.exe --list` gives it.
    Wsl { distro: String },
}

impl ExecutionTarget {
    pub fn is_this_machine(&self) -> bool {
        *self == ExecutionTarget::ThisMachine
    }
}

/// A Key for a Project named `name` that no other Project uses (`excluding`
/// is the Project being edited, if any).
pub fn suggested_project_key(projects: &[Project], name: &str, excluding: Option<Id>) -> Key {
    Key::suggested_unique(name, "project", |key| {
        projects.iter().any(|project| Some(project.id) != excluding && project.key.as_str() == key)
    })
}

/// How a Service is named outside its Project, e.g. "My SaaS › API".
/// `None` if no Project has it.
pub fn service_label(projects: &[Project], service_id: Id) -> Option<String> {
    projects
        .iter()
        .find_map(|project| project.service(service_id).map(|service| format!("{} › {}", project.name, service.name)))
}

/// One independently manageable long-running command belonging to a Project.
///
/// The command line is run through the user's login shell (ADR-0003), so
/// there is no separate arguments field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Service {
    pub id: Id,
    pub name: String,
    pub key: Key,
    /// Relative to the Project's path; `.` is the Project root.
    pub working_directory: String,
    pub command_line: String,
    /// Plain variables. Secrets are listed in `secret_names`.
    pub environment: BTreeMap<String, String>,
    /// The Service's Secrets, sorted; their values are in the `SecretStore`.
    #[serde(default)]
    pub secret_names: Vec<String>,
    /// The Services of the same Project this one needs before it can start.
    #[serde(default)]
    pub dependencies: Vec<Id>,
    /// Start All skips this Service, unless a Service it starts depends on it.
    #[serde(default)]
    pub is_excluded_from_start_all: bool,
    /// The TCP port the Service listens on: it is Running once the Port
    /// accepts connections, and its command gets it as `PORT` (ADR-0006).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Where to open the Service, e.g. `http://localhost:3000/admin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether a Run may use another port when the Port is taken. Off for
    /// commands that ignore `$PORT` (ADR-0006).
    #[serde(default = "default_true")]
    pub port_can_change: bool,
}

fn default_true() -> bool {
    true
}

impl Service {
    /// The Service's layer of a Run's environment.
    pub fn variables(&self) -> Variables {
        Variables { values: self.environment.clone(), secret_names: self.secret_names.clone() }
    }

    pub(crate) fn set_variables(&mut self, variables: Variables) {
        self.environment = variables.values;
        self.secret_names = variables.secret_names;
    }

    /// Whether a command line mentions `$PORT`, so a moved port reaches it.
    pub fn command_line_uses_port(command_line: &str) -> bool {
        command_line.contains("$PORT") || command_line.contains("${PORT")
    }
}

/// A Service to add together with others in one save, e.g. a reviewed
/// Suggestion. It has no variables or Dependencies yet.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct NewService {
    pub name: String,
    pub key: Key,
    /// Relative to the Project; `.` is its root.
    pub working_directory: String,
    pub command_line: String,
    pub port: Option<i64>,
    pub url: Option<String>,
    pub port_can_change: bool,
    pub is_excluded_from_start_all: bool,
}

impl NewService {
    /// A Service in the Project root, with no Port or URL.
    pub fn new(name: impl Into<String>, key: Key, command_line: impl Into<String>) -> NewService {
        NewService {
            name: name.into(),
            key,
            working_directory: ".".into(),
            command_line: command_line.into(),
            port: None,
            url: None,
            port_can_change: true,
            is_excluded_from_start_all: false,
        }
    }
}

/// The variables of one layer of a Run's environment (a Project, a Service,
/// or an Environment Profile): plain values, plus the names of its Secrets,
/// whose values never leave the `SecretStore`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Variables {
    pub values: BTreeMap<String, String>,
    /// Sorted.
    pub secret_names: Vec<String>,
}

impl Variables {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.secret_names.is_empty()
    }

    /// Every variable name, plain and Secret, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.values.keys().cloned().chain(self.secret_names.iter().cloned()).collect();
        names.sort();
        names
    }
}

/// One variable as set from an editor. A Secret's value is written to the
/// `SecretStore`; it is never kept in the Project Configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct VariableInput {
    pub name: String,
    pub value: VariableValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum VariableValue {
    Plain(String),
    Secret(String),
    /// A Secret that keeps its stored value, possibly under a new name.
    ExistingSecret {
        previous_name: String,
    },
}

impl VariableInput {
    pub fn plain(name: impl Into<String>, value: impl Into<String>) -> VariableInput {
        VariableInput { name: name.into(), value: VariableValue::Plain(value.into()) }
    }

    pub fn secret(name: impl Into<String>, value: impl Into<String>) -> VariableInput {
        VariableInput { name: name.into(), value: VariableValue::Secret(value.into()) }
    }

    /// The layer's Secret `name`, unchanged.
    pub fn existing_secret(name: impl Into<String>) -> VariableInput {
        let name = name.into();
        VariableInput { value: VariableValue::ExistingSecret { previous_name: name.clone() }, name }
    }

    /// The layer's Secret `previous_name`, keeping its value under `name`.
    pub fn renamed_secret(name: impl Into<String>, previous_name: impl Into<String>) -> VariableInput {
        VariableInput {
            name: name.into(),
            value: VariableValue::ExistingSecret { previous_name: previous_name.into() },
        }
    }

    /// Plain variables, sorted by name.
    pub fn plain_all<K: Into<String>, V: Into<String>>(values: impl IntoIterator<Item = (K, V)>) -> Vec<VariableInput> {
        let mut inputs: Vec<VariableInput> = values.into_iter().map(|(k, v)| VariableInput::plain(k, v)).collect();
        inputs.sort_by(|a, b| a.name.cmp(&b.name));
        inputs
    }
}

/// Which layer of a Run's environment variables belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum VariableLayer {
    Project(Id),
    Service(Id),
    Profile(Id),
}

impl VariableLayer {
    fn owner_id(&self) -> Id {
        match self {
            VariableLayer::Project(id) | VariableLayer::Service(id) | VariableLayer::Profile(id) => *id,
        }
    }

    /// Where the layer's Secret `name` is stored, e.g. `<SERVICE-ID>/API_KEY`.
    pub fn secret_account(&self, name: &str) -> String {
        format!("{}/{name}", self.owner_id())
    }
}

/// A named set of environment variables for a Project (e.g. Development,
/// Testing) applying to all its Services; at most one is active at a time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EnvironmentProfile {
    pub id: Id,
    pub name: String,
    pub variables: Variables,
}

impl EnvironmentProfile {
    pub(crate) fn new(id: Id, name: String, variables: Variables) -> EnvironmentProfile {
        EnvironmentProfile { id, name, variables }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_services_saved_before_dependencies_existed() {
        let json = r#"{"id":"6F9A2C1E-0000-4000-8000-000000000001","name":"Web","key":"web","workingDirectory":".","commandLine":"pnpm dev","environment":{}}"#;
        let service: Service = serde_json::from_str(json).unwrap();
        assert!(service.dependencies.is_empty());
        assert!(!service.is_excluded_from_start_all);
        assert!(service.port_can_change);
        assert_eq!(service.port, None);
    }

    #[test]
    fn secret_accounts_use_the_uppercase_owner_id() {
        let id: Id = "e852ef9f-3371-4b3f-8ba3-61dc7adf142c".parse().unwrap();
        assert_eq!(
            VariableLayer::Service(id).secret_account("STRIPE_KEY"),
            "E852EF9F-3371-4B3F-8BA3-61DC7ADF142C/STRIPE_KEY"
        );
    }

    #[test]
    fn knows_whether_a_command_line_uses_port() {
        assert!(Service::command_line_uses_port("next dev -p $PORT"));
        assert!(Service::command_line_uses_port("vite --port ${PORT:-5173}"));
        assert!(!Service::command_line_uses_port("docker compose up postgres"));
    }

    #[test]
    fn suggests_project_keys_no_other_project_uses() {
        let projects = [Project::with_services("My SaaS", &[]), Project::with_services("Blog", &[])];
        assert_eq!(suggested_project_key(&projects, "My SaaS", None).as_str(), "my-saas-2");
        // Editing a Project may keep its own Key.
        assert_eq!(suggested_project_key(&projects, "My SaaS", Some(projects[0].id)).as_str(), "my-saas");
        assert_eq!(suggested_project_key(&projects, "?", None).as_str(), "project");
    }

    #[test]
    fn suggests_service_keys_no_other_service_of_the_project_uses() {
        let project = Project::with_services("My SaaS", &[("API", None), ("Web", None)]);
        assert_eq!(project.suggested_service_key("api", None).as_str(), "api-2");
        assert_eq!(project.suggested_service_key("API", Some(project.services[0].id)).as_str(), "api");
        assert_eq!(project.suggested_service_key("Worker", None).as_str(), "worker");
    }

    #[test]
    fn labels_a_service_with_its_project() {
        let projects = [Project::with_services("My SaaS", &[("API", None)])];
        assert_eq!(service_label(&projects, projects[0].services[0].id).as_deref(), Some("My SaaS › API"));
        assert_eq!(service_label(&projects, Id::new()), None);
    }
}
