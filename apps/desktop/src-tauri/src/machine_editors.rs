//! The editors, on another Machine: the same sheets as for this computer's
//! Projects (`editors`), whose changes are made on the Machine, as
//! `Origin::Machine`. Its folders are browsed with `machine_list_directory`,
//! since no system dialog here can. What changed comes back with the
//! Machine's Projects (`machines`); nothing of it is kept here, and a
//! Secret typed here goes to the Machine's secret store only.
//!
//! An older localhost there refuses with `MachineNeedsUpdate`, and a lost
//! connection with `MachineUnavailable`: nothing waits to be sent later.

use localhost_core::machine::Machines;
use localhost_core::{
    DirectoryListing, ExecutionTarget, Id, Key, LocalhostError, Project, ProjectChanges, ProjectDocument, Service,
    ServiceStatus, ServiceSuggestion, VariableLayer,
};

use crate::AppState;
use crate::bridge::CommandResult;
use crate::dialogs::ProjectFolder;
use crate::editors::{
    ImportChoice, ImportPreview, ProfileEdit, ServiceEdit, ServiceRelations, SuggestionEdit, VariableEdit, file_name,
    inputs, new_services, parse_key, preview, profile_inputs, relations,
};

/// The Machine's Projects, as last read from it.
fn projects(machines: &Machines, machine_id: Id) -> Result<Vec<Project>, LocalhostError> {
    machines.projects(machine_id)
}

fn project(machines: &Machines, machine_id: Id, project_id: Id) -> Result<Project, LocalhostError> {
    projects(machines, machine_id)?
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or(LocalhostError::ProjectNotFound(project_id))
}

/// A folder of the Machine, for the folder picker: its folders, and with
/// `include_files` its files. `None` is the home folder there.
#[tauri::command]
#[specta::specta]
pub async fn machine_list_directory(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    path: Option<String>,
    show_hidden: bool,
    include_files: bool,
) -> CommandResult<DirectoryListing> {
    Ok(state.machines()?.list_directory(machine_id, path, show_hidden, include_files).await?)
}

/// `path` relative to the Project's folder `base`, e.g. `web` or
/// `../shared`, for a working directory picked on a Machine.
#[tauri::command]
#[specta::specta]
pub fn relative_folder(path: String, base: String) -> Option<String> {
    crate::dialogs::relative_path(std::path::Path::new(&path), std::path::Path::new(&base))
}

#[tauri::command]
#[specta::specta]
pub fn machine_suggested_project_key(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    name: String,
    excluding: Option<Id>,
) -> CommandResult<Key> {
    let projects = projects(state.machines()?, machine_id)?;
    Ok(localhost_core::suggested_project_key(&projects, &name, excluding))
}

#[tauri::command]
#[specta::specta]
pub fn machine_suggested_service_key(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    name: String,
    excluding: Option<Id>,
) -> CommandResult<Key> {
    Ok(project(state.machines()?, machine_id, project_id)?.suggested_service_key(&name, excluding))
}

// MARK: Projects

/// Adds a Project in the Machine's folder `path`, with the reviewed
/// Suggestions as its first Services.
#[tauri::command]
#[specta::specta]
pub async fn machine_add_project(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    name: String,
    key: String,
    path: String,
    services: Vec<SuggestionEdit>,
) -> CommandResult<Project> {
    let services = new_services(services)?;
    Ok(state.machines()?.add_project(machine_id, name, parse_key(&key)?, path, services).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_edit_project(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    name: String,
    key: String,
    path: String,
) -> CommandResult {
    let changes =
        ProjectChanges { name: Some(name), key: Some(parse_key(&key)?), path: Some(path), ..Default::default() };
    state.machines()?.edit_project(machine_id, project_id, changes).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn machine_set_favorite(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    is_favorite: bool,
) -> CommandResult {
    let changes = ProjectChanges { is_favorite: Some(is_favorite), ..Default::default() };
    state.machines()?.edit_project(machine_id, project_id, changes).await?;
    Ok(())
}

/// Stops the Project's Services there, then removes it and their logs. Its
/// folder is not touched.
#[tauri::command]
#[specta::specta]
pub async fn machine_remove_project(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
) -> CommandResult {
    Ok(state.machines()?.remove_project(machine_id, project_id).await?)
}

// MARK: Services

#[tauri::command]
#[specta::specta]
pub async fn machine_add_service(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    service: ServiceEdit,
) -> CommandResult<Service> {
    let (new, variables, dependencies) = service.new_service()?;
    Ok(state.machines()?.add_service(machine_id, project_id, new, variables, dependencies).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_edit_service(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    service_id: Id,
    service: ServiceEdit,
) -> CommandResult {
    state.machines()?.edit_service(machine_id, service_id, service.changes()?).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn machine_add_services(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    services: Vec<SuggestionEdit>,
) -> CommandResult<Vec<Service>> {
    let services = new_services(services)?;
    Ok(state.machines()?.add_services(machine_id, project_id, services).await?)
}

/// Stops the Service there, then removes it and its logs.
#[tauri::command]
#[specta::specta]
pub async fn machine_remove_service(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    service_id: Id,
) -> CommandResult {
    Ok(state.machines()?.remove_service(machine_id, service_id).await?)
}

/// The Dependencies the Service can't have, and who depends on it.
#[tauri::command]
#[specta::specta]
pub fn machine_service_relations(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    service_id: Id,
) -> CommandResult<ServiceRelations> {
    let projects = projects(state.machines()?, machine_id)?;
    let project = projects
        .iter()
        .find(|project| project.service(service_id).is_some())
        .ok_or(LocalhostError::ServiceNotFound(service_id))?;
    Ok(relations(project, service_id))
}

// MARK: Detection

/// Suggestions for a folder of the Machine, scanned there.
#[tauri::command]
#[specta::specta]
pub async fn machine_scan_folder(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    path: String,
) -> CommandResult<Vec<ServiceSuggestion>> {
    Ok(state.machines()?.scan_folder(machine_id, path).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_scan_project(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
) -> CommandResult<Vec<ServiceSuggestion>> {
    Ok(state.machines()?.scan_project(machine_id, project_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_detect_python_environment(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    working_directory: String,
    command_line: String,
) -> CommandResult<Option<String>> {
    let machines = state.machines()?;
    Ok(machines.detect_python_environment(machine_id, project_id, working_directory, command_line).await?)
}

// MARK: Environment

#[tauri::command]
#[specta::specta]
pub async fn machine_save_environment(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    variables: Vec<VariableEdit>,
    profiles: Vec<ProfileEdit>,
    active: Option<u32>,
) -> CommandResult {
    let (variables, profiles) = (inputs(variables), profile_inputs(profiles));
    Ok(state.machines()?.save_environment(machine_id, project_id, variables, profiles, active).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_set_active_profile(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    project_id: Id,
    profile_id: Option<Id>,
) -> CommandResult {
    Ok(state.machines()?.set_active_profile(machine_id, project_id, profile_id).await?)
}

/// A Secret's value there, to reveal it in the editor. Not kept here.
#[tauri::command]
#[specta::specta]
pub async fn machine_secret_value(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    name: String,
    layer: VariableLayer,
) -> CommandResult<Option<String>> {
    Ok(state.machines()?.secret_value(machine_id, name, layer).await?)
}

// MARK: Import

/// Reads a Project file of the Machine for the import's review, among the
/// Machine's Projects.
#[tauri::command]
#[specta::specta]
pub async fn machine_read_project_file(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    file: String,
) -> CommandResult<ImportPreview> {
    let machines = state.machines()?;
    let read = machines.read_project_file(machine_id, file.clone()).await?;
    let document = ProjectDocument::from_yaml(&read.text)?;
    let folder = ProjectFolder { target: ExecutionTarget::ThisMachine, path: document.path().to_string() };
    let (snapshots, _) = machines.subscribe();
    let statuses: Vec<ServiceStatus> = snapshots
        .into_iter()
        .find(|snapshot| snapshot.machine.id == machine_id)
        .map(|snapshot| snapshot.statuses)
        .unwrap_or_default();
    let is_active = |id: Id| statuses.iter().any(|status| status.service_id == id && status.state.is_active());
    let projects = projects(machines, machine_id)?;
    Ok(preview(&projects, is_active, &document, file_name(&file), folder, read.folder_exists))
}

/// Imports the Machine's Project file into its folder `path` there, read
/// again as Import is pressed.
#[tauri::command]
#[specta::specta]
pub async fn machine_import_project(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    file: String,
    path: String,
    choice: Option<ImportChoice>,
) -> CommandResult<Project> {
    let machines = state.machines()?;
    let read = machines.read_project_file(machine_id, file).await?;
    Ok(machines.import_project(machine_id, read.text, path, choice.map(Into::into)).await?)
}
