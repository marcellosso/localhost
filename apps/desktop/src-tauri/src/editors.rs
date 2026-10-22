//! The editors: adding, editing and removing Projects, Services and
//! Environment Profiles, Secrets, Detect Services, and Import / Export.
//!
//! The UI sends what the user typed; this module parses it into the core's
//! types (Keys, Ports, variables), so a mistake comes back in the core's
//! words. Removing stops what is running first.

use std::collections::HashSet;

use localhost_core::{
    Id, Key, KeyClash, LocalhostError, NewService, Origin, ProfileChanges, Project, ProjectChanges, ProjectDocument,
    Service, ServiceChanges, ServiceSuggestion, VariableInput, VariableLayer,
};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::bridge::{CommandError, CommandResult};
use crate::dialogs::ProjectFolder;
use crate::{AppState, Core};

const ORIGIN: Origin = Origin::MainWindow;

/// One variable row as the user left it.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariableEdit {
    name: String,
    value: VariableEditValue,
}

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VariableEditValue {
    Plain {
        value: String,
    },
    /// A Secret with a value typed (or revealed) in the editor.
    Secret {
        value: String,
    },
    /// A stored Secret whose value wasn't touched, possibly renamed.
    #[serde(rename_all = "camelCase")]
    StoredSecret {
        previous_name: String,
    },
}

impl From<VariableEdit> for VariableInput {
    fn from(edit: VariableEdit) -> VariableInput {
        match edit.value {
            VariableEditValue::Plain { value } => VariableInput::plain(edit.name, value),
            VariableEditValue::Secret { value } => VariableInput::secret(edit.name, value),
            VariableEditValue::StoredSecret { previous_name } => {
                VariableInput::renamed_secret(edit.name, previous_name)
            }
        }
    }
}

fn inputs(variables: Vec<VariableEdit>) -> Vec<VariableInput> {
    variables.into_iter().map(VariableInput::from).collect()
}

/// A reviewed Suggestion to add, as edited.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionEdit {
    name: String,
    key: String,
    working_directory: String,
    command_line: String,
    /// As typed; empty for none.
    port: String,
    port_can_change: bool,
}

/// The Service editor's fields.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEdit {
    name: String,
    key: String,
    working_directory: String,
    command_line: String,
    /// As typed; empty for none.
    port: String,
    /// Empty for none.
    url: String,
    port_can_change: bool,
    variables: Vec<VariableEdit>,
    dependencies: Vec<Id>,
    is_excluded_from_start_all: bool,
}

/// One Environment Profile as edited; `profile_id` is `None` for a new one.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEdit {
    profile_id: Option<Id>,
    name: String,
    variables: Vec<VariableEdit>,
}

/// What a Service's editor needs to know about the others of its Project.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRelations {
    /// The other Services it can't depend on, with the cycle that would make,
    /// e.g. "api → worker → api would be a dependency cycle."
    cycles: Vec<Cycle>,
    /// The names of the Services that depend on it.
    dependents: Vec<String>,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Cycle {
    service_id: Id,
    message: String,
}

/// The review step of an import: what the file brings, read again on Import.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    file_name: String,
    name: String,
    key: Key,
    /// The folder the file names; it may not exist here.
    folder: ProjectFolder,
    folder_exists: bool,
    services: Vec<ImportedService>,
    /// e.g. "2 project variables · profiles Development (active), Testing".
    environment: Option<String>,
    /// Every Secret the file names, which has no value until it is set.
    secrets: Vec<ImportedSecret>,
    /// The Project already using the file's Key, if any.
    clash: Option<ImportClash>,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedService {
    name: String,
    key: Key,
    working_directory: String,
    command_line: String,
    port: Option<u16>,
    is_excluded_from_start_all: bool,
    dependencies: Vec<Key>,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSecret {
    name: String,
    /// "project", the Service's name, or "profile Testing".
    owner: String,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportClash {
    project_name: String,
    /// The Key "Keep both" imports it under.
    free_key: Key,
    /// What Replace removes, e.g. "Web (running, stopped first)"; empty if nothing.
    removed: Vec<String>,
}

/// What to do when the imported Key is taken.
#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportChoice {
    KeepBoth,
    Replace,
}

// MARK: Keys

/// A Key derived from `name` that no other Project uses.
#[tauri::command]
#[specta::specta]
pub fn suggested_project_key(
    state: tauri::State<'_, AppState>,
    name: String,
    excluding: Option<Id>,
) -> CommandResult<Key> {
    Ok(localhost_core::suggested_project_key(&state.core()?.library.projects(), &name, excluding))
}

/// A Key derived from `name` that no other Service of the Project uses.
#[tauri::command]
#[specta::specta]
pub fn suggested_service_key(
    state: tauri::State<'_, AppState>,
    project_id: Id,
    name: String,
    excluding: Option<Id>,
) -> CommandResult<Key> {
    Ok(state.core()?.library.project(project_id)?.suggested_service_key(&name, excluding))
}

/// A Key derived from a Suggestion's name; empty if it has no usable characters.
#[tauri::command]
#[specta::specta]
pub fn key_from_name(name: String) -> String {
    Key::suggested_from(&name).map(|key| key.to_string()).unwrap_or_default()
}

// MARK: Projects

/// Adds a Project with the reviewed Suggestions as its first Services.
#[tauri::command]
#[specta::specta]
pub fn add_project(
    state: tauri::State<'_, AppState>,
    name: String,
    key: String,
    folder: ProjectFolder,
    services: Vec<SuggestionEdit>,
) -> CommandResult<Project> {
    let services = new_services(services)?;
    let library = &state.core()?.library;
    Ok(library.add_project_on(folder.target, &name, parse_key(&key)?, &folder.path, services, ORIGIN)?)
}

/// Saves the Project's editor. A Project moved to another Execution Target
/// has its Services stopped first, and their logs on the old one removed.
#[tauri::command]
#[specta::specta]
pub async fn edit_project(
    state: tauri::State<'_, AppState>,
    project_id: Id,
    name: String,
    key: String,
    folder: ProjectFolder,
) -> CommandResult {
    let core = state.core()?;
    let project = core.library.project(project_id)?;
    let moves = project.execution_target != folder.target;
    if moves {
        for service in &project.services {
            core.targets.stop(service.id, ORIGIN).await?;
        }
    }
    let changes = ProjectChanges {
        name: Some(name),
        key: Some(parse_key(&key)?),
        path: Some(folder.path),
        execution_target: Some(folder.target),
        ..Default::default()
    };
    core.library.edit_project(project_id, changes, ORIGIN)?;
    if moves {
        for service in &project.services {
            core.targets.remove_logs(&project.execution_target, service.id).await?;
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_favorite(state: tauri::State<'_, AppState>, project_id: Id, is_favorite: bool) -> CommandResult {
    let changes = ProjectChanges { is_favorite: Some(is_favorite), ..Default::default() };
    state.core()?.library.edit_project(project_id, changes, ORIGIN)?;
    Ok(())
}

/// Stops the Project's Services, then removes it and their logs. The
/// folder is not touched.
#[tauri::command]
#[specta::specta]
pub async fn remove_project(state: tauri::State<'_, AppState>, project_id: Id) -> CommandResult {
    let core = state.core()?;
    let project = core.library.project(project_id)?;
    for service in &project.services {
        core.targets.stop(service.id, ORIGIN).await?;
    }
    core.library.remove_project(project_id, ORIGIN)?;
    for service in &project.services {
        core.targets.remove_logs(&project.execution_target, service.id).await?;
    }
    Ok(())
}

// MARK: Services

#[tauri::command]
#[specta::specta]
pub fn add_service(state: tauri::State<'_, AppState>, project_id: Id, service: ServiceEdit) -> CommandResult<Service> {
    let new = NewService {
        name: service.name,
        key: parse_key(&service.key)?,
        working_directory: service.working_directory,
        command_line: service.command_line,
        port: parse_port(&service.port, None)?,
        url: Some(service.url),
        port_can_change: service.port_can_change,
        is_excluded_from_start_all: service.is_excluded_from_start_all,
    };
    let library = &state.core()?.library;
    Ok(library.add_service(project_id, new, inputs(service.variables), service.dependencies, ORIGIN)?)
}

#[tauri::command]
#[specta::specta]
pub fn edit_service(state: tauri::State<'_, AppState>, service_id: Id, service: ServiceEdit) -> CommandResult {
    let changes = ServiceChanges {
        name: Some(service.name),
        key: Some(parse_key(&service.key)?),
        working_directory: Some(service.working_directory),
        command_line: Some(service.command_line),
        variables: Some(inputs(service.variables)),
        dependencies: Some(service.dependencies),
        is_excluded_from_start_all: Some(service.is_excluded_from_start_all),
        port: Some(parse_port(&service.port, None)?),
        url: Some(Some(service.url)),
        port_can_change: Some(service.port_can_change),
    };
    state.core()?.library.edit_service(service_id, changes, ORIGIN)?;
    Ok(())
}

/// Whether the command line mentions `$PORT` (`%PORT%` on Windows), so a moved port reaches it.
#[tauri::command]
#[specta::specta]
pub fn command_line_uses_port(command_line: String) -> bool {
    Service::command_line_uses_port(&command_line)
}

/// Adds the reviewed Suggestions of Detect Services in one save.
#[tauri::command]
#[specta::specta]
pub fn add_services(
    state: tauri::State<'_, AppState>,
    project_id: Id,
    services: Vec<SuggestionEdit>,
) -> CommandResult<Vec<Service>> {
    let services = new_services(services)?;
    Ok(state.core()?.library.add_services(services, project_id, ORIGIN)?)
}

/// Stops the Service, then removes it and its logs.
#[tauri::command]
#[specta::specta]
pub async fn remove_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    let core = state.core()?;
    let (project, _) = core.library.service(service_id)?;
    core.targets.stop(service_id, ORIGIN).await?;
    core.library.remove_service(service_id, ORIGIN)?;
    core.targets.remove_logs(&project.execution_target, service_id).await?;
    Ok(())
}

/// The Dependencies the Service can't have, and who depends on it.
#[tauri::command]
#[specta::specta]
pub fn service_relations(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult<ServiceRelations> {
    let (project, _) = state.core()?.library.service(service_id)?;
    Ok(relations(&project, service_id))
}

fn relations(project: &Project, service_id: Id) -> ServiceRelations {
    let cycles = project
        .services
        .iter()
        .filter(|other| other.id != service_id)
        .filter_map(|other| {
            let keys = project.dependency_cycle(service_id, other.id)?;
            Some(Cycle { service_id: other.id, message: LocalhostError::DependencyCycle(keys).message() })
        })
        .collect();
    let dependents = project.dependents(service_id).iter().map(|service| service.name.clone()).collect();
    ServiceRelations { cycles, dependents }
}

// MARK: Detection

/// Suggestions for a folder being added as a Project, scanned where it
/// will run: in a distro, by its `localhostd`.
#[tauri::command]
#[specta::specta]
pub async fn scan_folder(
    state: tauri::State<'_, AppState>,
    folder: ProjectFolder,
) -> CommandResult<Vec<ServiceSuggestion>> {
    Ok(state.core()?.targets.scan_folder(&folder.target, &folder.path).await?)
}

/// Suggestions for an existing Project, where it runs, without what its
/// Services cover.
#[tauri::command]
#[specta::specta]
pub async fn scan_project(state: tauri::State<'_, AppState>, project_id: Id) -> CommandResult<Vec<ServiceSuggestion>> {
    let core = state.core()?;
    let project = core.library.project(project_id)?;
    Ok(core.targets.scan_project(&project).await?)
}

// MARK: Environment

/// Saves the Environment editor: the Project's variables, its Profiles
/// (added, edited, removed), then which one is active (an index into
/// `profiles`, or none).
#[tauri::command]
#[specta::specta]
pub fn save_environment(
    state: tauri::State<'_, AppState>,
    project_id: Id,
    variables: Vec<VariableEdit>,
    profiles: Vec<ProfileEdit>,
    active: Option<u32>,
) -> CommandResult {
    let library = &state.core()?.library;
    let project = library.project(project_id)?;
    let changes = ProjectChanges { variables: Some(inputs(variables)), ..Default::default() };
    library.edit_project(project_id, changes, ORIGIN)?;
    let kept: HashSet<Id> = profiles.iter().filter_map(|profile| profile.profile_id).collect();
    for profile in project.profiles.iter().filter(|profile| !kept.contains(&profile.id)) {
        library.remove_profile(profile.id, ORIGIN)?;
    }
    let mut active_id = None;
    for (index, edit) in profiles.into_iter().enumerate() {
        let variables = inputs(edit.variables);
        let id = match edit.profile_id {
            Some(id) => {
                let changes = ProfileChanges { name: Some(edit.name), variables: Some(variables) };
                library.edit_profile(id, changes, ORIGIN)?.id
            }
            None => library.add_profile(project_id, &edit.name, variables, ORIGIN)?.id,
        };
        if active == Some(index as u32) {
            active_id = Some(id);
        }
    }
    library.set_active_profile(active_id, project_id, ORIGIN)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_active_profile(state: tauri::State<'_, AppState>, project_id: Id, profile_id: Option<Id>) -> CommandResult {
    state.core()?.library.set_active_profile(profile_id, project_id, ORIGIN)?;
    Ok(())
}

/// A stored Secret's value, to reveal it; `None` if it has none.
#[tauri::command]
#[specta::specta]
pub fn secret_value(
    state: tauri::State<'_, AppState>,
    name: String,
    layer: VariableLayer,
) -> CommandResult<Option<String>> {
    Ok(state.core()?.library.secret_value(&name, layer)?)
}

// MARK: Import and Export

/// Reads a Project file for the import's review step.
#[tauri::command]
#[specta::specta]
pub fn read_project_file(state: tauri::State<'_, AppState>, file: String) -> CommandResult<ImportPreview> {
    let core = state.core()?;
    let document = read_document(&file)?;
    Ok(preview(core, &document, file_name(&file)))
}

/// Imports the Project file into `folder`. On Replace, Services the file no
/// longer has are stopped first, and their logs removed after; all of them
/// if the Project moves to another Execution Target.
#[tauri::command]
#[specta::specta]
pub async fn import_project(
    state: tauri::State<'_, AppState>,
    file: String,
    folder: ProjectFolder,
    choice: Option<ImportChoice>,
) -> CommandResult<Project> {
    let core = state.core()?;
    let mut document = read_document(&file)?;
    document.set_path(folder.path);
    document.set_execution_target(folder.target);
    let on_key_clash = choice.map(|choice| match choice {
        ImportChoice::KeepBoth => KeyClash::KeepBoth,
        ImportChoice::Replace => KeyClash::Replace,
    });
    let replaced = match on_key_clash {
        Some(KeyClash::Replace) => clashing(core, &document),
        _ => None,
    };
    let moves = |existing: &Project| existing.execution_target != *document.execution_target();
    if let Some(existing) = &replaced {
        let stopped: Vec<&Service> = if moves(existing) {
            existing.services.iter().collect()
        } else {
            removed_by(existing, &document).collect()
        };
        for service in stopped {
            core.targets.stop(service.id, ORIGIN).await?;
        }
    }
    let project = core.library.import_project(&document, on_key_clash, ORIGIN)?;
    if let Some(existing) = &replaced {
        let gone = |service: &&Service| moves(existing) || project.service(service.id).is_none();
        for service in existing.services.iter().filter(gone) {
            core.targets.remove_logs(&existing.execution_target, service.id).await?;
        }
    }
    Ok(project)
}

/// Writes the Project's file to `file`.
#[tauri::command]
#[specta::specta]
pub fn export_project(state: tauri::State<'_, AppState>, project_id: Id, file: String) -> CommandResult {
    let project = state.core()?.library.project(project_id)?;
    std::fs::write(&file, ProjectDocument::from(&project).to_yaml())
        .map_err(|error| LocalhostError::PersistenceFailed { path: file, reason: error.to_string() })?;
    Ok(())
}

fn read_document(file: &str) -> Result<ProjectDocument, LocalhostError> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| LocalhostError::PersistenceFailed { path: file.to_string(), reason: error.to_string() })?;
    ProjectDocument::from_yaml(&text)
}

fn file_name(file: &str) -> String {
    std::path::Path::new(file).file_name().map_or_else(|| file.to_string(), |name| name.to_string_lossy().into_owned())
}

fn clashing(core: &Core, document: &ProjectDocument) -> Option<Project> {
    core.library.projects().into_iter().find(|project| project.key == *document.key())
}

/// The existing Project's Services that the file doesn't have.
fn removed_by<'a>(existing: &'a Project, document: &ProjectDocument) -> impl Iterator<Item = &'a Service> {
    let kept: HashSet<Key> = document.services().iter().map(|service| service.key.clone()).collect();
    existing.services.iter().filter(move |service| !kept.contains(&service.key))
}

fn preview(core: &Core, document: &ProjectDocument, file_name: String) -> ImportPreview {
    let clash = clashing(core, document).map(|existing| ImportClash {
        project_name: existing.name.clone(),
        free_key: core.library.free_project_key(document.key()),
        removed: removed_by(&existing, document)
            .map(|service| {
                if core.targets.status(service.id).state.is_active() {
                    format!("{} (running, stopped first)", service.name)
                } else {
                    service.name.clone()
                }
            })
            .collect(),
    });
    let services = document
        .services()
        .iter()
        .map(|service| ImportedService {
            name: service.name.clone(),
            key: service.key.clone(),
            working_directory: service.working_directory.clone(),
            command_line: service.command_line.clone(),
            port: service.port,
            is_excluded_from_start_all: service.is_excluded_from_start_all,
            dependencies: service.dependencies.clone(),
        })
        .collect();
    let secret = |name: &String, owner: String| ImportedSecret { name: name.clone(), owner };
    let secrets =
        document
            .variables()
            .secret_names
            .iter()
            .map(|name| secret(name, "project".into()))
            .chain(document.services().iter().flat_map(|service| {
                service.variables.secret_names.iter().map(|name| secret(name, service.name.clone()))
            }))
            .chain(document.profiles().iter().flat_map(|profile| {
                profile.variables.secret_names.iter().map(|name| secret(name, format!("profile {}", profile.name)))
            }))
            .collect();
    let folder = ProjectFolder { target: document.execution_target().clone(), path: document.path().to_string() };
    ImportPreview {
        file_name,
        name: document.name().to_string(),
        key: document.key().clone(),
        folder_exists: folder.exists(),
        folder,
        services,
        environment: environment_summary(document),
        secrets,
        clash,
    }
}

/// e.g. "2 project variables · profiles Development (active), Testing".
fn environment_summary(document: &ProjectDocument) -> Option<String> {
    let mut parts = Vec::new();
    match document.variables().values.len() {
        0 => {}
        1 => parts.push("1 project variable".to_string()),
        count => parts.push(format!("{count} project variables")),
    }
    let profiles = document.profiles();
    if !profiles.is_empty() {
        let names: Vec<String> = profiles
            .iter()
            .map(|profile| {
                if Some(profile.name.as_str()) == document.active_profile() {
                    format!("{} (active)", profile.name)
                } else {
                    profile.name.clone()
                }
            })
            .collect();
        let label = if profiles.len() == 1 { "profile" } else { "profiles" };
        parts.push(format!("{label} {}", names.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

// MARK: Parsing

fn parse_key(raw: &str) -> Result<Key, LocalhostError> {
    Key::new(raw.trim())
}

/// A port as typed; empty is none. `service` names it in the message, for
/// a list of Suggestions.
fn parse_port(raw: &str, service: Option<&str>) -> Result<Option<i64>, CommandError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed.parse::<i64>().map(Some).map_err(|_| {
        let message = match service {
            Some(name) => format!("The port of “{name}” must be a number, e.g. 3000."),
            None => "The port must be a number, e.g. 3000.".to_string(),
        };
        CommandError::new(LocalhostError::InvalidPort(0), message)
    })
}

fn new_services(edits: Vec<SuggestionEdit>) -> Result<Vec<NewService>, CommandError> {
    edits
        .into_iter()
        .map(|edit| {
            Ok(NewService {
                port: parse_port(&edit.port, Some(&edit.name))?,
                working_directory: edit.working_directory,
                port_can_change: edit.port_can_change,
                ..NewService::new(edit.name, parse_key(&edit.key)?, edit.command_line)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use localhost_core::MemorySecretStore;

    use super::*;

    fn core() -> (Core, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("localhost-editors-{}-{}", std::process::id(), Id::new()));
        std::fs::create_dir_all(&root).unwrap();
        (Core::open_with_secrets(&root, Arc::new(MemorySecretStore::new())).unwrap(), root)
    }

    fn suggestion(name: &str, key: &str, port: &str) -> SuggestionEdit {
        SuggestionEdit {
            name: name.into(),
            key: key.into(),
            working_directory: ".".into(),
            command_line: "sleep 30".into(),
            port: port.into(),
            port_can_change: true,
        }
    }

    #[test]
    fn a_port_that_is_not_a_number_is_named_in_the_message() {
        let error = new_services(vec![suggestion("Web", "web", "30o0")]).unwrap_err();
        assert_eq!(error.message, "The port of “Web” must be a number, e.g. 3000.");
        assert_eq!(parse_port(" ", None).unwrap(), None);
        assert_eq!(parse_port("3000", None).unwrap(), Some(3000));
    }

    #[test]
    fn variable_edits_become_the_cores_inputs() {
        let edit = |value| VariableEdit { name: "API_KEY".into(), value };
        assert_eq!(
            VariableInput::from(edit(VariableEditValue::StoredSecret { previous_name: "KEY".into() })),
            VariableInput::renamed_secret("API_KEY", "KEY")
        );
        assert_eq!(
            VariableInput::from(edit(VariableEditValue::Secret { value: "s3cret".into() })),
            VariableInput::secret("API_KEY", "s3cret")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn says_which_dependencies_would_be_cycles_and_who_depends_on_a_service() {
        let (core, root) = core();
        let path = root.to_string_lossy();
        let services = new_services(vec![suggestion("API", "api", ""), suggestion("Worker", "worker", "")]).unwrap();
        let project = core.library.add_project("Shop", Key::new("shop").unwrap(), &path, services, ORIGIN).unwrap();
        let (api, worker) = (project.services[0].id, project.services[1].id);
        let changes = ServiceChanges { dependencies: Some(vec![api]), ..Default::default() };
        core.library.edit_service(worker, changes, ORIGIN).unwrap();

        let project = core.library.project(project.id).unwrap();
        let relations = relations(&project, api);
        assert_eq!(relations.dependents, ["Worker"]);
        assert_eq!(relations.cycles.len(), 1);
        assert_eq!(relations.cycles[0].service_id, worker);
        assert_eq!(relations.cycles[0].message, "api → worker → api would be a dependency cycle.");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn previews_an_import_that_clashes_with_an_existing_project() {
        let (core, root) = core();
        let path = root.to_string_lossy();
        let services = new_services(vec![suggestion("Web", "web", "3000")]).unwrap();
        let project = core.library.add_project("Shop", Key::new("shop").unwrap(), &path, services, ORIGIN).unwrap();
        let changes = ProjectChanges {
            variables: Some(vec![VariableInput::plain("MODE", "dev"), VariableInput::secret("TOKEN", "t")]),
            ..Default::default()
        };
        core.library.edit_project(project.id, changes, ORIGIN).unwrap();
        core.library.add_profile(project.id, "Testing", Vec::new(), ORIGIN).unwrap();
        let mut document = ProjectDocument::from(&core.library.project(project.id).unwrap());
        document.set_path("/no/such/folder");
        // Added after the export, so Replace would remove it.
        core.library
            .add_services(new_services(vec![suggestion("Docs", "docs", "")]).unwrap(), project.id, ORIGIN)
            .unwrap();

        let preview = preview(&core, &document, "shop.yaml".into());
        assert!(!preview.folder_exists);
        assert_eq!(preview.services.len(), 1);
        assert_eq!(preview.environment.as_deref(), Some("1 project variable · profile Testing"));
        assert_eq!(preview.secrets.len(), 1);
        assert_eq!((preview.secrets[0].name.as_str(), preview.secrets[0].owner.as_str()), ("TOKEN", "project"));
        let clash = preview.clash.expect("shop is taken");
        assert_eq!(clash.project_name, "Shop");
        assert_eq!(clash.free_key.as_str(), "shop-2");
        assert_eq!(clash.removed, ["Docs"]);
        let _ = std::fs::remove_dir_all(root);
    }
}
