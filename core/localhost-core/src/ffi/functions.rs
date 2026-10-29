//! Free functions over the core's value types. UniFFI records carry data
//! only, so the Swift layer wraps these in extensions (`error.message`, `project.dependents(of:)`, …).

use crate::{
    Ansi, FuzzyMatch, Id, Key, Listener, LocalUrl, LocalhostError, Orphan, PortWarning, Project, ProjectFileProblem,
    ProjectScanner, Run, Service, ServiceState, ServiceStatus, ServiceSuggestion, StyledLine, Termination,
    UnreadableProjectFile, Variables,
};

use super::runtime::blocking;

#[uniffi::export]
pub fn core_version() -> String {
    crate::VERSION.to_string()
}

/// Where Projects, logs and Run records live by default.
#[uniffi::export]
pub fn default_storage_root() -> String {
    crate::default_storage_root().to_string_lossy().into_owned()
}

/// Serves the shared core of `storage_root` until it is idle: what the app's
/// executable does when started with `--daemon <storage root>`. Returns why
/// it couldn't, or `None`.
#[uniffi::export]
pub fn run_shared_core(storage_root: String) -> Option<String> {
    crate::remote::shared::run(std::path::Path::new(&storage_root)).err()
}

// MARK: Errors and endings

#[uniffi::export]
pub fn localhost_error_message(error: &LocalhostError) -> String {
    error.message()
}

#[uniffi::export]
pub fn project_file_problem_message(problem: &ProjectFileProblem) -> String {
    problem.message()
}

#[uniffi::export]
pub fn termination_summary(termination: Termination) -> String {
    termination.summary()
}

#[uniffi::export]
pub fn signal_name(signal: i32) -> String {
    crate::signal_name(signal)
}

// MARK: Keys

#[uniffi::export]
pub fn key_new(raw: String) -> Result<Key, LocalhostError> {
    Key::new(raw)
}

#[uniffi::export]
pub fn key_suggested_from(name: &str) -> Option<Key> {
    Key::suggested_from(name)
}

#[uniffi::export]
pub fn suggested_project_key(projects: Vec<Project>, name: &str, excluding: Option<Id>) -> Key {
    crate::suggested_project_key(&projects, name, excluding)
}

#[uniffi::export]
pub fn project_suggested_service_key(project: &Project, name: &str, excluding: Option<Id>) -> Key {
    project.suggested_service_key(name, excluding)
}

// MARK: Projects and Services

#[uniffi::export]
pub fn project_dependents(project: &Project, service_id: Id) -> Vec<Service> {
    project.dependents(service_id).into_iter().cloned().collect()
}

#[uniffi::export]
pub fn project_dependency_cycle(project: &Project, service_id: Id, dependency_id: Id) -> Option<Vec<Key>> {
    project.dependency_cycle(service_id, dependency_id)
}

#[uniffi::export]
pub fn project_dependency_order(project: &Project, service_ids: Vec<Id>) -> Vec<Id> {
    project.dependency_order(&service_ids)
}

#[uniffi::export]
pub fn project_working_directory(project: &Project, service: &Service) -> String {
    project.working_directory(service).to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn service_url_to_open(service: &Service, run: Option<Run>) -> Option<String> {
    service.url_to_open(run.as_ref())
}

#[uniffi::export]
pub fn command_line_uses_port(command_line: &str) -> bool {
    Service::command_line_uses_port(command_line)
}

#[uniffi::export]
pub fn variables_names(variables: &Variables) -> Vec<String> {
    variables.names()
}

#[uniffi::export]
pub fn service_state_is_active(state: ServiceState) -> bool {
    state.is_active()
}

#[uniffi::export]
pub fn service_state_most_significant(states: Vec<ServiceState>) -> Option<ServiceState> {
    ServiceState::most_significant(states)
}

#[uniffi::export]
pub fn service_status_never_run(service_id: Id) -> ServiceStatus {
    ServiceStatus::never_run(service_id)
}

#[uniffi::export]
pub fn listener_is_unmanaged(listener: &Listener) -> bool {
    listener.is_unmanaged()
}

#[uniffi::export]
pub fn listener_reason_not_stoppable(listener: &Listener) -> Option<LocalhostError> {
    listener.reason_not_stoppable()
}

#[uniffi::export]
pub fn listener_owner_description(listener: &Listener, projects: Vec<Project>) -> String {
    listener.owner_description(&projects)
}

#[uniffi::export]
pub fn port_warnings(
    projects: Vec<Project>,
    statuses: Vec<ServiceStatus>,
    listeners: Vec<Listener>,
) -> Vec<PortWarning> {
    crate::port_warnings(&projects, &statuses, &listeners)
}

#[uniffi::export]
pub fn orphan_description(orphan: &Orphan, projects: Vec<Project>) -> String {
    orphan.description(&projects)
}

#[uniffi::export]
pub fn unreadable_project_file_description(file: &UnreadableProjectFile) -> String {
    file.description()
}

// MARK: Detection

/// Suggestions for the folder at `path`, being added as a Project.
#[uniffi::export]
pub async fn scan_folder(path: String) -> Vec<ServiceSuggestion> {
    blocking(move || ProjectScanner::default().scan_folder(&path)).await
}

/// Suggestions for an existing Project, without what its Services cover.
#[uniffi::export]
pub async fn scan_project(project: Project) -> Vec<ServiceSuggestion> {
    blocking(move || ProjectScanner::default().scan_project(&project)).await
}

// MARK: Text

#[uniffi::export]
pub fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    FuzzyMatch::score(query, text)
}

#[uniffi::export]
pub fn ansi_parse(line: &str) -> StyledLine {
    Ansi::parse(line)
}

#[uniffi::export]
pub fn ansi_strip(line: &str) -> String {
    Ansi::strip(line)
}

/// A local URL found in a line of output. `start` and `end` are UTF-8 byte
/// offsets into the text.
#[derive(uniffi::Record)]
pub struct LocalUrlMatch {
    pub start: u64,
    pub end: u64,
    pub url: String,
}

#[uniffi::export]
pub fn local_url_matches(text: &str) -> Vec<LocalUrlMatch> {
    LocalUrl::matches(text)
        .into_iter()
        .map(|(range, url)| LocalUrlMatch { start: range.start as u64, end: range.end as u64, url })
        .collect()
}

#[uniffi::export]
pub fn local_url_first(text: &str) -> Option<String> {
    LocalUrl::first(text)
}
