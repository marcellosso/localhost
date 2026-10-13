//! What crosses to the UI: a snapshot of the Projects and Service statuses,
//! then the core's own events as diffs over a channel. Statuses carry their
//! failure in the core's words, so the UI never words errors itself.

use std::collections::HashMap;

use localhost_core::{Id, LocalhostError, Project, ProjectLibraryEvent, ServiceEvent, ServiceState, ServiceStatus};
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;

use crate::AppState;

/// Everything the UI shows, at the moment it subscribed.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    projects: Vec<Project>,
    /// Services that never ran have no status: they are Stopped.
    statuses: Vec<StatusView>,
    project_states: Vec<ProjectState>,
}

/// A Service's status with the message of its Run's failure, if any.
#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    #[serde(flatten)]
    status: ServiceStatus,
    failure_message: Option<String>,
}

impl StatusView {
    fn new(status: ServiceStatus) -> StatusView {
        let failure_message = status.run.as_ref().and_then(|run| run.failure.as_ref()).map(LocalhostError::message);
        StatusView { status, failure_message }
    }
}

/// A Project's most significant Service state (the sidebar glyph); `None`
/// for a Project without Services.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectState {
    project_id: Id,
    state: Option<ServiceState>,
}

/// A command's failure: the core's typed error, and its message for the user.
#[derive(Debug, Serialize, Type)]
pub struct CommandError {
    pub error: LocalhostError,
    pub message: String,
}

pub type CommandResult<T = ()> = Result<T, CommandError>;

impl CommandError {
    /// A failure the app itself words, e.g. about the editor it couldn't open.
    pub fn new(error: LocalhostError, message: impl Into<String>) -> CommandError {
        CommandError { error, message: message.into() }
    }
}

impl From<LocalhostError> for CommandError {
    fn from(error: LocalhostError) -> CommandError {
        CommandError { message: error.message(), error }
    }
}

#[derive(Clone, Serialize, Type)]
#[serde(tag = "kind", content = "event", rename_all = "camelCase")]
pub enum CoreEvent {
    Library(ProjectLibraryEvent),
    /// A Service's state changed. (Runs starting and ending show up here too.)
    Status(StatusView),
    /// Sent after the event that changed it.
    ProjectState(ProjectState),
}

/// The snapshot, then every change over `events` until the window goes away.
#[tauri::command]
#[specta::specta]
pub fn subscribe(state: tauri::State<'_, AppState>, events: Channel<CoreEvent>) -> Result<Snapshot, CommandError> {
    let core = state.core()?;
    let (projects, mut library_events) = core.library.subscribe();
    let (statuses, mut service_events) = core.services.subscribe();
    let mut view = View { projects, statuses, states: HashMap::new() };
    let project_states: Vec<ProjectState> = view.projects.iter().map(|project| view.project_state(project)).collect();
    view.states = project_states.iter().map(|state| (state.project_id, state.state)).collect();
    let snapshot = Snapshot {
        projects: view.projects.clone(),
        statuses: view.statuses.values().cloned().map(StatusView::new).collect(),
        project_states,
    };

    tauri::async_runtime::spawn(async move {
        loop {
            let (event, project_id) = tokio::select! {
                Some(event) = library_events.recv() => {
                    let project_id = view.apply_library(&event);
                    (CoreEvent::Library(event), project_id)
                }
                Some(event) = service_events.recv() => {
                    let ServiceEvent::ServiceStateChanged(status) = event else { continue };
                    let project_id = view.apply_status(&status);
                    (CoreEvent::Status(StatusView::new(status)), project_id)
                }
                else => return,
            };
            // A send fails once the window is closed or reloaded.
            if events.send(event).is_err() {
                return;
            }
            if let Some(changed) = project_id.and_then(|id| view.refresh_state(id))
                && events.send(CoreEvent::ProjectState(changed)).is_err()
            {
                return;
            }
        }
    });
    Ok(snapshot)
}

/// One subscriber's copy of the state, for the derived Project states.
struct View {
    projects: Vec<Project>,
    statuses: HashMap<Id, ServiceStatus>,
    states: HashMap<Id, Option<ServiceState>>,
}

impl View {
    /// Applies the event; returns the Project whose state may have changed.
    fn apply_library(&mut self, event: &ProjectLibraryEvent) -> Option<Id> {
        match event {
            ProjectLibraryEvent::ProjectAdded { project, .. } => {
                self.projects.push(project.clone());
                Some(project.id)
            }
            ProjectLibraryEvent::ProjectUpdated { project, .. } => {
                if let Some(existing) = self.projects.iter_mut().find(|existing| existing.id == project.id) {
                    *existing = project.clone();
                }
                Some(project.id)
            }
            ProjectLibraryEvent::ProjectRemoved { id, .. } => {
                self.projects.retain(|project| project.id != *id);
                self.states.remove(id);
                None
            }
        }
    }

    fn apply_status(&mut self, status: &ServiceStatus) -> Option<Id> {
        self.statuses.insert(status.service_id, status.clone());
        self.projects.iter().find(|project| project.service(status.service_id).is_some()).map(|project| project.id)
    }

    /// The Project's state if it differs from the last one sent.
    fn refresh_state(&mut self, project_id: Id) -> Option<ProjectState> {
        let project = self.projects.iter().find(|project| project.id == project_id)?;
        let state = self.project_state(project);
        (self.states.insert(project_id, state.state) != Some(state.state)).then_some(state)
    }

    fn project_state(&self, project: &Project) -> ProjectState {
        let states = project
            .services
            .iter()
            .map(|service| self.statuses.get(&service.id).map_or(ServiceState::Stopped, |status| status.state));
        ProjectState { project_id: project.id, state: ServiceState::most_significant(states) }
    }
}
