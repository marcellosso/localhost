//! Project files: a Project's Configuration as one YAML file, written by
//! Export and read by Import.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::paths::abbreviate_tilde;
use crate::yaml::{YamlEntry, YamlKind, YamlValue, write_scalar};
use crate::{ExecutionTarget, Id, Key, LocalhostError, Project, Service, Variables, validate};

/// A Project's Configuration as a Project file, so it can move between
/// machines (and later into a repo or a CLI). It has no ids, sort order or
/// favourite, and Secrets are listed by name only: their values never leave
/// the `SecretStore`.
///
/// A document read from YAML is validated with the editors' rules, so
/// [`ProjectLibrary::import_project`](crate::ProjectLibrary::import_project)
/// only fails on what changed since: the path the user picked, or a Key that
/// became taken.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct ProjectDocument {
    name: String,
    key: Key,
    path: String,
    execution_target: ExecutionTarget,
    variables: Variables,
    services: Vec<DocumentService>,
    profiles: Vec<DocumentProfile>,
    active_profile: Option<String>,
}

/// A Service of a [`ProjectDocument`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct DocumentService {
    pub name: String,
    pub key: Key,
    /// Relative to the Project's path; `.` is the Project root.
    pub working_directory: String,
    pub command_line: String,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub port_can_change: bool,
    pub is_excluded_from_start_all: bool,
    /// The Keys of the Services of the same file it needs, in order.
    pub dependencies: Vec<Key>,
    pub variables: Variables,
}

/// An Environment Profile of a [`ProjectDocument`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct DocumentProfile {
    pub name: String,
    pub variables: Variables,
}

impl From<&Project> for ProjectDocument {
    fn from(project: &Project) -> ProjectDocument {
        ProjectDocument {
            name: project.name.clone(),
            key: project.key.clone(),
            path: project.path.clone(),
            execution_target: project.execution_target.clone(),
            variables: project.variables.clone(),
            services: project
                .services
                .iter()
                .map(|service| DocumentService {
                    name: service.name.clone(),
                    key: service.key.clone(),
                    working_directory: service.working_directory.clone(),
                    command_line: service.command_line.clone(),
                    port: service.port,
                    url: service.url.clone(),
                    port_can_change: service.port_can_change,
                    is_excluded_from_start_all: service.is_excluded_from_start_all,
                    dependencies: service
                        .dependencies
                        .iter()
                        .filter_map(|id| project.service(*id).map(|dependency| dependency.key.clone()))
                        .collect(),
                    variables: service.variables(),
                })
                .collect(),
            profiles: project
                .profiles
                .iter()
                .map(|profile| DocumentProfile { name: profile.name.clone(), variables: profile.variables.clone() })
                .collect(),
            active_profile: project.active_profile().map(|profile| profile.name.clone()),
        }
    }
}

impl ProjectDocument {
    /// The `localhost:` value of files this version writes and reads.
    pub const FORMAT_VERSION: i64 = 1;

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key(&self) -> &Key {
        &self.key
    }

    /// The absolute path of the Project's folder.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Points the import elsewhere, e.g. where the folder is on this
    /// machine. Validated on import.
    pub fn set_path(&mut self, path: impl Into<String>) {
        self.path = path.into();
    }

    /// Where the Project ran on the machine that wrote the file.
    pub fn execution_target(&self) -> &ExecutionTarget {
        &self.execution_target
    }

    /// Runs the import elsewhere, e.g. on this machine when the file came
    /// from a WSL distro. Validated on import, with the path.
    pub fn set_execution_target(&mut self, target: ExecutionTarget) {
        self.execution_target = target;
    }

    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    pub fn services(&self) -> &[DocumentService] {
        &self.services
    }

    pub fn profiles(&self) -> &[DocumentProfile] {
        &self.profiles
    }

    /// The name of the active Environment Profile, if any.
    pub fn active_profile(&self) -> Option<&str> {
        self.active_profile.as_deref()
    }

    // MARK: Writing

    /// The Project file. Fields with their default value are left out, and
    /// variables are sorted, so the same Configuration always gives the same text.
    pub fn to_yaml(&self) -> String {
        let mut lines = vec![
            format!("localhost: {}", Self::FORMAT_VERSION),
            format!("name: {}", write_scalar(&self.name)),
            format!("key: {}", write_scalar(self.key.as_str())),
        ];
        match &self.execution_target {
            ExecutionTarget::ThisMachine => {
                lines.push(format!("path: {}", write_scalar(&abbreviate_tilde(&self.path))));
            }
            // `~` would stand for this machine's home folder, not the distro's.
            ExecutionTarget::Wsl { distro } => {
                lines.push(format!("wsl: {}", write_scalar(distro)));
                lines.push(format!("path: {}", write_scalar(&self.path)));
            }
        }
        lines.extend(variable_lines(&self.variables, ""));
        if !self.services.is_empty() {
            lines.push("services:".into());
            for service in &self.services {
                lines.push(format!("  {}:", write_scalar(service.key.as_str())));
                lines.push(format!("    name: {}", write_scalar(&service.name)));
                lines.push(format!("    command: {}", write_scalar(&service.command_line)));
                lines.push(format!("    cwd: {}", write_scalar(&service.working_directory)));
                if let Some(port) = service.port {
                    lines.push(format!("    port: {port}"));
                }
                if let Some(url) = &service.url {
                    lines.push(format!("    url: {}", write_scalar(url)));
                }
                if !service.port_can_change {
                    lines.push("    port_can_change: false".into());
                }
                if service.is_excluded_from_start_all {
                    lines.push("    start_all: false".into());
                }
                if !service.dependencies.is_empty() {
                    lines.push("    depends_on:".into());
                    lines.extend(
                        service.dependencies.iter().map(|key| format!("      - {}", write_scalar(key.as_str()))),
                    );
                }
                lines.extend(variable_lines(&service.variables, "    "));
            }
        }
        if !self.profiles.is_empty() {
            lines.push("profiles:".into());
            for profile in &self.profiles {
                let variables = variable_lines(&profile.variables, "    ");
                let empty = if variables.is_empty() { " {}" } else { "" };
                lines.push(format!("  {}:{empty}", write_scalar(&profile.name)));
                lines.extend(variables);
            }
        }
        if let Some(active_profile) = &self.active_profile {
            lines.push(format!("active_profile: {}", write_scalar(active_profile)));
        }
        lines.join("\n") + "\n"
    }

    // MARK: Reading

    /// Reads and validates a Project file. Errors are
    /// [`LocalhostError::InvalidProjectFile`], naming the field and line.
    pub fn from_yaml(text: &str) -> Result<ProjectDocument, LocalhostError> {
        let root = YamlValue::parse(text)?;
        let version = match &root.kind {
            YamlKind::Mapping(entries) => entries.iter().find(|entry| entry.key == "localhost"),
            _ => None,
        };
        let Some(version) = version else {
            return Err(LocalhostError::InvalidProjectFile {
                problem: ProjectFileProblem::NotAProjectFile,
                field: Vec::new(),
                line: None,
            });
        };
        let version_error = |problem| LocalhostError::InvalidProjectFile {
            problem,
            field: vec!["localhost".into()],
            line: Some(version.line),
        };
        let number = match &version.value.kind {
            YamlKind::Scalar(raw) => raw.parse::<i64>().ok().filter(|number| *number >= 1),
            _ => None,
        }
        .ok_or_else(|| version_error(ProjectFileProblem::Expected(ValueKind::Number)))?;
        if number > Self::FORMAT_VERSION {
            return Err(version_error(ProjectFileProblem::NewerFormat(number)));
        }

        let mut file = Fields::new(&root, Vec::new(), None)?;
        file.entry("localhost");
        let name = file.required("name", |raw| Ok(validate::name(raw)?))?;
        let key = file.required("key", |raw| Ok(Key::new(raw)?))?;
        let execution_target = file
            .optional("wsl", |raw| Ok(validate::execution_target(ExecutionTarget::Wsl { distro: raw.into() })?))?
            .unwrap_or_default();
        let path = file.required("path", |raw| Ok(validate::project_path(raw, &execution_target)?))?;
        let variables = read_variables(&mut file)?;

        let mut services = Vec::new();
        let mut dependency_lists = Vec::new();
        if let Some(entry) = file.entry("services") {
            for service_entry in Fields::new(&entry.value, vec!["services".into()], Some(entry.line))?.entries {
                let (service, dependencies) = read_service(service_entry)?;
                services.push(service);
                dependency_lists.push(dependencies);
            }
        }
        resolve_dependencies(&dependency_lists, &mut services)?;

        let mut profiles: Vec<DocumentProfile> = Vec::new();
        if let Some(entry) = file.entry("profiles") {
            for profile_entry in Fields::new(&entry.value, vec!["profiles".into()], Some(entry.line))?.entries {
                let field = vec!["profiles".to_string(), profile_entry.key.clone()];
                let name = validate::name(&profile_entry.key)
                    .and_then(|name| match profiles.iter().any(|profile| profile.name == name) {
                        true => Err(LocalhostError::DuplicateProfileName(name)),
                        false => Ok(name),
                    })
                    .map_err(|error| invalid(error, field.clone(), profile_entry.line))?;
                let mut fields = Fields::new(&profile_entry.value, field, Some(profile_entry.line))?;
                let variables = read_variables(&mut fields)?;
                fields.finish()?;
                profiles.push(DocumentProfile { name, variables });
            }
        }

        let active_profile = file.optional("active_profile", |raw| {
            let name = raw.trim();
            match profiles.iter().any(|profile| profile.name == name) {
                true => Ok(name.to_string()),
                false => Err(ProjectFileProblem::UnknownProfile(name.to_string())),
            }
        })?;
        file.finish()?;
        Ok(ProjectDocument { name, key, path, execution_target, variables, services, profiles, active_profile })
    }
}

/// A layer's `variables:` (name → value) and `secrets:` (names) lines.
fn variable_lines(variables: &Variables, indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    if !variables.values.is_empty() {
        lines.push(format!("{indent}variables:"));
        lines.extend(
            variables
                .values
                .iter()
                .map(|(name, value)| format!("{indent}  {}: {}", write_scalar(name), write_scalar(value))),
        );
    }
    if !variables.secret_names.is_empty() {
        lines.push(format!("{indent}secrets:"));
        lines.extend(variables.secret_names.iter().map(|name| format!("{indent}  - {}", write_scalar(name))));
    }
    lines
}

/// A `depends_on` item: the raw Key and its line.
type DependencyItem = (String, usize);

fn read_service(entry: &YamlEntry) -> Result<(DocumentService, Vec<DependencyItem>), LocalhostError> {
    let field = vec!["services".to_string(), entry.key.clone()];
    let key = Key::new(entry.key.as_str()).map_err(|error| invalid(error, field.clone(), entry.line))?;
    let mut fields = Fields::new(&entry.value, field, Some(entry.line))?;
    let service = DocumentService {
        name: fields.optional("name", |raw| Ok(validate::name(raw)?))?.unwrap_or_else(|| entry.key.clone()),
        key,
        working_directory: fields.optional("cwd", |raw| Ok(validate::working_directory(raw)?))?.unwrap_or(".".into()),
        command_line: fields.required("command", |raw| Ok(validate::command_line(raw)?))?,
        port: fields
            .optional("port", |raw| {
                let port = raw.parse::<i64>().map_err(|_| ProjectFileProblem::Expected(ValueKind::Number))?;
                Ok(validate::port(Some(port))?)
            })?
            .flatten(),
        url: fields.optional("url", |raw| Ok(validate::url(Some(raw))?))?.flatten(),
        port_can_change: fields.boolean("port_can_change")?.unwrap_or(true),
        is_excluded_from_start_all: !fields.boolean("start_all")?.unwrap_or(true),
        dependencies: Vec::new(),
        variables: read_variables(&mut fields)?,
    };
    let dependencies = fields.list("depends_on")?;
    fields.finish()?;
    Ok((service, dependencies))
}

/// Turns the `depends_on` Keys into Dependencies, rejecting unknown Keys and
/// cycles as the Service editor does. Repeated Keys are dropped.
fn resolve_dependencies(lists: &[Vec<DependencyItem>], services: &mut [DocumentService]) -> Result<(), LocalhostError> {
    // A scratch Project to reuse the editor's cycle check.
    let mut project = Project::new(String::new(), Key::new("scratch")?, "/".into(), 0);
    project.services = services
        .iter()
        .map(|service| Service {
            id: Id::new(),
            name: service.name.clone(),
            key: service.key.clone(),
            working_directory: service.working_directory.clone(),
            command_line: service.command_line.clone(),
            environment: BTreeMap::new(),
            secret_names: Vec::new(),
            dependencies: Vec::new(),
            is_excluded_from_start_all: false,
            port: None,
            url: None,
            port_can_change: true,
        })
        .collect();
    for (index, list) in lists.iter().enumerate() {
        let field = vec!["services".to_string(), services[index].key.to_string(), "depends_on".into()];
        for (raw, line) in list {
            let Some(dependency) = project.services.iter().find(|service| service.key.as_str() == raw) else {
                return Err(LocalhostError::InvalidProjectFile {
                    problem: ProjectFileProblem::UnknownDependency(raw.clone()),
                    field,
                    line: Some(*line),
                });
            };
            let (dependency_id, dependency_key) = (dependency.id, dependency.key.clone());
            if project.services[index].dependencies.contains(&dependency_id) {
                continue;
            }
            if let Some(cycle) = project.dependency_cycle(project.services[index].id, dependency_id) {
                return Err(invalid(LocalhostError::DependencyCycle(cycle), field, *line));
            }
            project.services[index].dependencies.push(dependency_id);
            services[index].dependencies.push(dependency_key);
        }
    }
    Ok(())
}

/// A layer's `variables` (name → value) and `secrets` (names).
fn read_variables(fields: &mut Fields) -> Result<Variables, LocalhostError> {
    let mut values = BTreeMap::new();
    if let Some(entry) = fields.entry("variables") {
        let variables_field = [fields.field.clone(), vec!["variables".into()]].concat();
        let variables = Fields::new(&entry.value, variables_field, Some(entry.line))?;
        for variable in variables.entries {
            let field = [variables.field.clone(), vec![variable.key.clone()]].concat();
            if !validate::is_valid_variable_name(&variable.key) {
                return Err(invalid(
                    LocalhostError::InvalidEnvironmentVariableName(variable.key.clone()),
                    field,
                    variable.line,
                ));
            }
            let value = match &variable.value.kind {
                YamlKind::Scalar(value) => value.clone(),
                YamlKind::Null => String::new(),
                YamlKind::Sequence(_) | YamlKind::Mapping(_) => {
                    return Err(LocalhostError::InvalidProjectFile {
                        problem: ProjectFileProblem::Expected(ValueKind::Text),
                        field,
                        line: Some(variable.line),
                    });
                }
            };
            values.insert(variable.key.clone(), value);
        }
    }
    let mut secret_names: Vec<String> = Vec::new();
    for (name, line) in fields.list("secrets")? {
        let problem = if !validate::is_valid_variable_name(&name) {
            Some(LocalhostError::InvalidEnvironmentVariableName(name.clone()))
        } else if values.contains_key(&name) || secret_names.contains(&name) {
            Some(LocalhostError::DuplicateVariableName(name.clone()))
        } else {
            None
        };
        if let Some(problem) = problem {
            return Err(invalid(problem, [fields.field.clone(), vec!["secrets".into()]].concat(), line));
        }
        secret_names.push(name);
    }
    secret_names.sort();
    Ok(Variables { values, secret_names })
}

/// `error`, which breaks an editor's rule, reported at `field` and `line`.
fn invalid(error: LocalhostError, field: Vec<String>, line: usize) -> LocalhostError {
    LocalhostError::InvalidProjectFile {
        problem: ProjectFileProblem::Invalid(Box::new(error)),
        field,
        line: Some(line),
    }
}

/// The fields of one mapping of a Project file, read once each; whatever is
/// left when it is finished is an unknown field.
struct Fields<'a> {
    field: Vec<String>,
    line: usize,
    entries: &'a [YamlEntry],
    used: HashSet<&'a str>,
}

impl<'a> Fields<'a> {
    /// `Null` (e.g. `Testing:` with nothing under it) reads as no fields.
    fn new(value: &'a YamlValue, field: Vec<String>, line: Option<usize>) -> Result<Fields<'a>, LocalhostError> {
        let line = line.unwrap_or(value.line);
        let entries: &[YamlEntry] = match &value.kind {
            YamlKind::Mapping(entries) => entries,
            YamlKind::Null => &[],
            YamlKind::Scalar(_) | YamlKind::Sequence(_) => {
                return Err(LocalhostError::InvalidProjectFile {
                    problem: ProjectFileProblem::Expected(ValueKind::Mapping),
                    field,
                    line: Some(line),
                });
            }
        };
        Ok(Fields { field, line, entries, used: HashSet::new() })
    }

    fn entry(&mut self, key: &str) -> Option<&'a YamlEntry> {
        let entry = self.entries.iter().find(|entry| entry.key == key)?;
        self.used.insert(&entry.key);
        Some(entry)
    }

    fn error(&self, problem: ProjectFileProblem, key: &str, line: usize) -> LocalhostError {
        LocalhostError::InvalidProjectFile {
            problem,
            field: [self.field.clone(), vec![key.into()]].concat(),
            line: Some(line),
        }
    }

    /// A text field's value, transformed (and validated) by `transform`.
    fn optional<T>(
        &mut self,
        key: &str,
        transform: impl FnOnce(&str) -> Result<T, ProjectFileProblem>,
    ) -> Result<Option<T>, LocalhostError> {
        let Some(entry) = self.entry(key) else { return Ok(None) };
        let YamlKind::Scalar(raw) = &entry.value.kind else {
            return Err(self.error(ProjectFileProblem::Expected(ValueKind::Text), key, entry.line));
        };
        transform(raw).map(Some).map_err(|problem| self.error(problem, key, entry.line))
    }

    fn required<T>(
        &mut self,
        key: &str,
        transform: impl FnOnce(&str) -> Result<T, ProjectFileProblem>,
    ) -> Result<T, LocalhostError> {
        self.optional(key, transform)?.ok_or_else(|| self.error(ProjectFileProblem::Missing, key, self.line))
    }

    fn boolean(&mut self, key: &str) -> Result<Option<bool>, LocalhostError> {
        self.optional(key, |raw| match raw {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ProjectFileProblem::Expected(ValueKind::Boolean)),
        })
    }

    /// A list of text items with their lines; empty when absent.
    fn list(&mut self, key: &str) -> Result<Vec<(String, usize)>, LocalhostError> {
        let Some(entry) = self.entry(key) else { return Ok(Vec::new()) };
        match &entry.value.kind {
            YamlKind::Null => Ok(Vec::new()),
            YamlKind::Sequence(items) => items
                .iter()
                .map(|item| match &item.kind {
                    YamlKind::Scalar(raw) => Ok((raw.clone(), item.line)),
                    _ => Err(self.error(ProjectFileProblem::Expected(ValueKind::Text), key, item.line)),
                })
                .collect(),
            YamlKind::Scalar(_) | YamlKind::Mapping(_) => {
                Err(self.error(ProjectFileProblem::Expected(ValueKind::List), key, entry.line))
            }
        }
    }

    fn finish(&self) -> Result<(), LocalhostError> {
        match self.entries.iter().find(|entry| !self.used.contains(entry.key.as_str())) {
            Some(unknown) => Err(self.error(ProjectFileProblem::UnknownField, &unknown.key, unknown.line)),
            None => Ok(()),
        }
    }
}

/// What is wrong in a Project file; see [`LocalhostError::InvalidProjectFile`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ProjectFileProblem {
    /// The file has no `localhost:` version line.
    NotAProjectFile,
    /// Written by a newer localhost, in this format version.
    NewerFormat(i64),
    ExpectedKeyValue,
    UnexpectedIndentation,
    UnterminatedQuote,
    InvalidEscape,
    /// YAML this reader doesn't take, e.g. "anchors and aliases".
    UnsupportedYaml(String),
    Duplicate,
    Missing,
    UnknownField,
    Expected(ValueKind),
    /// A `depends_on` Key that no Service of the file has.
    UnknownDependency(String),
    /// An `active_profile` that names no Profile of the file.
    UnknownProfile(String),
    /// The value breaks an editor's rule (Key, port, URL, cycle, …).
    Invalid(Box<LocalhostError>),
}

/// The kind of value a Project file field takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ValueKind {
    Text,
    Number,
    Boolean,
    List,
    Mapping,
}

impl From<LocalhostError> for ProjectFileProblem {
    fn from(error: LocalhostError) -> ProjectFileProblem {
        ProjectFileProblem::Invalid(Box::new(error))
    }
}

impl ProjectFileProblem {
    pub fn message(&self) -> String {
        use ProjectFileProblem::*;
        match self {
            NotAProjectFile => "This isn’t a localhost project file: it has no “localhost: 1” line.".into(),
            NewerFormat(version) => {
                format!("This file was written by a newer version of localhost (format {version}).")
            }
            ExpectedKeyValue => "Expected “key: value” or “- item”.".into(),
            UnexpectedIndentation => "The indentation doesn’t match the lines around it.".into(),
            UnterminatedQuote => "A quoted value isn’t closed.".into(),
            InvalidEscape => "A double-quoted value has an unknown escape sequence.".into(),
            UnsupportedYaml(feature) => format!("localhost project files don’t support {feature}."),
            Duplicate => "Defined more than once.".into(),
            Missing => "This field is required.".into(),
            UnknownField => "localhost doesn’t know this field.".into(),
            Expected(ValueKind::Text) => "Expected text.".into(),
            Expected(ValueKind::Number) => "Expected a whole number.".into(),
            Expected(ValueKind::Boolean) => "Expected true or false.".into(),
            Expected(ValueKind::List) => "Expected a list.".into(),
            Expected(ValueKind::Mapping) => "Expected “key: value” lines.".into(),
            UnknownDependency(key) => format!("No service in this file has the key “{key}”."),
            UnknownProfile(name) => format!("No profile in this file is called “{name}”."),
            Invalid(error) => error.message(),
        }
    }
}
