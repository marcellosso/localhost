//! Other Machines: computers whose localhost this app shows, reached with
//! the system's `ssh`. The core keeps each one connected and mirrors its
//! Projects and statuses; this sends them to the UI as this computer's are
//! (`bridge`), each Machine with a view of its own, and turns its buttons
//! into calls on the Machine. Changes there are made as `Origin::Machine`,
//! which the other Machine decides.

use std::collections::HashMap;

use localhost_core::machine::{MachineSnapshot, MachineState, Machines, MachinesEvent, ssh_config_hosts};
use localhost_core::{Id, LocalhostError, Machine, ServiceEvent};
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;

use crate::AppState;
use crate::bridge::{CommandResult, ProjectState, StatusView, View};

/// One Machine as the UI shows it. Its Projects and statuses are those of
/// its last connection, kept while it reconnects.
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MachineView {
    machine: Machine,
    state: MachineState,
    /// How it is, in the core's words: "Connected · localhost 0.5.0", or why not.
    summary: String,
    is_connected: bool,
    /// It waits for the user to connect it again.
    needs_user: bool,
    /// Why its Projects can't be added, edited or removed now: it isn't
    /// connected, or its localhost is too old. `None` when they can.
    edit_problem: Option<String>,
    projects: Vec<localhost_core::Project>,
    statuses: Vec<StatusView>,
    project_states: Vec<ProjectState>,
}

#[derive(Clone, Serialize, Type)]
// Per variant, not `rename_all_fields`, which specta doesn't read.
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MachineEvent {
    /// Every Machine afresh: one was added, removed or renamed, or its
    /// state, Projects or statuses changed as a whole.
    Machines { machines: Vec<MachineView> },
    /// A Service's state changed there.
    #[serde(rename_all = "camelCase")]
    Status { machine_id: Id, status: Box<StatusView> },
    /// Sent after the status that changed it.
    #[serde(rename_all = "camelCase")]
    ProjectState { machine_id: Id, project_state: ProjectState },
}

/// The UI's copy of one Machine.
struct Shown {
    machine: Machine,
    state: MachineState,
    view: View,
}

impl Shown {
    fn new(snapshot: MachineSnapshot) -> Shown {
        let statuses = snapshot.statuses.into_iter().map(|status| (status.service_id, status)).collect();
        Shown { machine: snapshot.machine, state: snapshot.state, view: View::new(snapshot.projects, statuses) }
    }

    fn view(&self) -> MachineView {
        MachineView {
            summary: self.state.summary(&self.machine.name),
            is_connected: matches!(self.state, MachineState::Connected { .. }),
            needs_user: self.state.needs_user(),
            edit_problem: edit_problem(&self.state, &self.machine.name),
            machine: self.machine.clone(),
            state: self.state.clone(),
            projects: self.view.projects.clone(),
            statuses: self.view.status_views(),
            project_states: self.view.project_states(),
        }
    }
}

/// Why the Machine's Projects can't be changed from here now, if they can't.
fn edit_problem(state: &MachineState, machine: &str) -> Option<String> {
    match state {
        MachineState::Connected { manages_projects: true, .. } => None,
        MachineState::Connected { manages_projects: false, .. } => {
            Some(LocalhostError::MachineNeedsUpdate { machine: machine.to_string() }.message())
        }
        other => Some(other.summary(machine)),
    }
}

/// The Machines as they are, then every change over `events` until the
/// window goes away.
#[tauri::command]
#[specta::specta]
pub fn subscribe_machines(
    state: tauri::State<'_, AppState>,
    events: Channel<MachineEvent>,
) -> CommandResult<Vec<MachineView>> {
    let machines = state.machines()?;
    let (snapshots, mut changes) = machines.subscribe();
    let mut shown: Vec<Shown> = snapshots.into_iter().map(Shown::new).collect();
    let snapshot = shown.iter().map(Shown::view).collect();

    let task = tauri::async_runtime::spawn(async move {
        while let Some(change) = changes.recv().await {
            let sent = apply(&mut shown, change);
            for event in sent {
                // A send fails once the window is closed or reloaded.
                if events.send(event).is_err() {
                    return;
                }
            }
        }
    });
    state.subscriptions.push(task);
    Ok(snapshot)
}

/// Applies the change; returns what the UI is told.
fn apply(shown: &mut Vec<Shown>, change: MachinesEvent) -> Vec<MachineEvent> {
    let every = |shown: &Vec<Shown>| vec![MachineEvent::Machines { machines: shown.iter().map(Shown::view).collect() }];
    match change {
        MachinesEvent::MachinesChanged { machines } => {
            let mut previous: HashMap<Id, Shown> = shown.drain(..).map(|one| (one.machine.id, one)).collect();
            *shown = machines
                .into_iter()
                .map(|machine| match previous.remove(&machine.id) {
                    Some(one) => Shown { machine, ..one },
                    None => Shown {
                        machine,
                        state: MachineState::NotConnected,
                        view: View::new(Vec::new(), HashMap::new()),
                    },
                })
                .collect();
            every(shown)
        }
        MachinesEvent::StateChanged { machine_id, state } => {
            let Some(one) = shown.iter_mut().find(|one| one.machine.id == machine_id) else { return Vec::new() };
            one.state = state;
            every(shown)
        }
        MachinesEvent::ProjectsChanged { machine_id, projects } => {
            let Some(one) = shown.iter_mut().find(|one| one.machine.id == machine_id) else { return Vec::new() };
            one.view = View::new(projects, one.view.statuses().clone());
            every(shown)
        }
        MachinesEvent::StatusesChanged { machine_id, statuses } => {
            let Some(one) = shown.iter_mut().find(|one| one.machine.id == machine_id) else { return Vec::new() };
            let statuses = statuses.into_iter().map(|status| (status.service_id, status)).collect();
            one.view = View::new(std::mem::take(&mut one.view.projects), statuses);
            every(shown)
        }
        MachinesEvent::ServiceChanged { machine_id, event: ServiceEvent::ServiceStateChanged(status) } => {
            let Some(one) = shown.iter_mut().find(|one| one.machine.id == machine_id) else { return Vec::new() };
            let project_id = one.view.apply_status(&status);
            let mut sent = vec![MachineEvent::Status { machine_id, status: Box::new(one.view.status_view(&status)) }];
            sent.extend(
                project_id
                    .and_then(|id| one.view.refresh_state(id))
                    .map(|project_state| MachineEvent::ProjectState { machine_id, project_state }),
            );
            sent
        }
        // Runs starting and ending show up in the status changes too.
        MachinesEvent::ServiceChanged { .. } => Vec::new(),
    }
}

/// Connects to the Machine `ssh` reaches as `destination`, and only once
/// that works keeps it. Fails with why it can't be reached.
#[tauri::command]
#[specta::specta]
pub async fn add_machine(
    state: tauri::State<'_, AppState>,
    destination: String,
    name: Option<String>,
) -> CommandResult<Machine> {
    Ok(state.machines()?.add(name, &destination).await?)
}

#[tauri::command]
#[specta::specta]
pub fn rename_machine(state: tauri::State<'_, AppState>, machine_id: Id, name: String) -> CommandResult<Machine> {
    Ok(state.machines()?.rename(machine_id, &name)?)
}

/// Disconnects and forgets the Machine. Nothing changes on it.
#[tauri::command]
#[specta::specta]
pub async fn remove_machine(state: tauri::State<'_, AppState>, machine_id: Id) -> CommandResult {
    Ok(state.machines()?.remove(machine_id)?)
}

/// Connects again now: at once if it was waiting, else anew.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_machine(state: tauri::State<'_, AppState>, machine_id: Id) -> CommandResult {
    let machines = state.machines()?;
    match machines.state(machine_id) {
        MachineState::Connected { .. } => machines.reconnect(machine_id)?,
        _ => machines.connect(machine_id)?,
    }
    Ok(())
}

/// The hosts named in `~/.ssh/config`, to suggest as a destination. Only read.
#[tauri::command]
#[specta::specta]
pub async fn ssh_hosts() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(ssh_config_hosts).await.unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn machine_start_service(state: tauri::State<'_, AppState>, machine_id: Id, service_id: Id) -> CommandResult {
    state.machines()?.start(machine_id, service_id, None).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn machine_stop_service(state: tauri::State<'_, AppState>, machine_id: Id, service_id: Id) -> CommandResult {
    state.machines()?.stop(machine_id, service_id).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn machine_restart_service(
    state: tauri::State<'_, AppState>,
    machine_id: Id,
    service_id: Id,
) -> CommandResult {
    state.machines()?.restart(machine_id, service_id).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn machine_start_all(state: tauri::State<'_, AppState>, machine_id: Id, project_id: Id) -> CommandResult {
    Ok(state.machines()?.start_all(machine_id, project_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn machine_stop_all(state: tauri::State<'_, AppState>, machine_id: Id, project_id: Id) -> CommandResult {
    Ok(state.machines()?.stop_all(machine_id, project_id).await?)
}

/// Open in Browser: forwards the Service's Port to the same port here,
/// then opens its URL. Fails with `PortConflict` when that port is taken here.
#[tauri::command]
#[specta::specta]
pub async fn machine_open_service(state: tauri::State<'_, AppState>, machine_id: Id, service_id: Id) -> CommandResult {
    match state.machines()?.open_service(machine_id, service_id).await? {
        Some(url) => crate::desktop::open_url(url),
        None => Ok(()),
    }
}

/// Connects to every Machine as the app starts; each then keeps reconnecting.
pub fn connect_all(machines: &Machines) {
    for machine in machines.machines() {
        let _ = machines.connect(machine.id);
    }
}

/// Active Services on connected Machines started from a Machine: those this
/// computer may hold, which stop there once it disconnects, unless
/// localhost is open on that Machine.
pub fn active_count(machines: &Machines) -> usize {
    let (snapshots, _) = machines.subscribe();
    snapshots
        .iter()
        .filter(|snapshot| matches!(snapshot.state, MachineState::Connected { .. }))
        .flat_map(|snapshot| &snapshot.statuses)
        .filter(|status| {
            status.state.is_active()
                && status.run.as_ref().is_some_and(|run| run.origin == localhost_core::Origin::Machine)
        })
        .count()
}

impl AppState {
    pub fn machines(&self) -> Result<&Machines, LocalhostError> {
        self.machines.as_ref().map_err(Clone::clone)
    }
}

#[cfg(test)]
mod tests {
    use localhost_core::{Key, NewService, Project, ServiceState, ServiceStatus};

    use super::*;

    fn project() -> Project {
        let root = std::env::temp_dir().join(format!("localhost-machines-{}", Id::new()));
        let library =
            localhost_core::ProjectLibrary::open(&root, std::sync::Arc::new(localhost_core::MemorySecretStore::new()))
                .unwrap();
        let web = NewService { port: Some(3000), ..NewService::new("Web", Key::new("web").unwrap(), "pnpm dev") };
        let path = root.to_string_lossy().into_owned();
        let project = library
            .add_project("Shop", Key::new("shop").unwrap(), &path, vec![web], localhost_core::Origin::Cli)
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        project
    }

    #[test]
    fn shows_each_machine_as_this_computer_is_shown() {
        let devbox = Machine { id: Id::new(), name: "devbox".into(), destination: "devbox".into() };
        let mut shown = Vec::new();
        let sent = apply(&mut shown, MachinesEvent::MachinesChanged { machines: vec![devbox.clone()] });
        let [MachineEvent::Machines { machines }] = sent.as_slice() else { panic!("not every Machine") };
        assert_eq!((machines[0].summary.as_str(), machines[0].needs_user), ("Not connected", true));
        assert_eq!(machines[0].edit_problem.as_deref(), Some("Not connected"));

        let project = project();
        let web = project.services[0].clone();
        apply(&mut shown, MachinesEvent::ProjectsChanged { machine_id: devbox.id, projects: vec![project.clone()] });
        let state = MachineState::Connected { version: "0.5.6".into(), manages_projects: false };
        let sent = apply(&mut shown, MachinesEvent::StateChanged { machine_id: devbox.id, state });
        let [MachineEvent::Machines { machines }] = sent.as_slice() else { panic!("not every Machine") };
        assert_eq!(machines[0].edit_problem.as_deref(), Some("Update localhost on devbox to manage its Projects."));
        let state = MachineState::Connected { version: "0.5.7".into(), manages_projects: true };
        let sent = apply(&mut shown, MachinesEvent::StateChanged { machine_id: devbox.id, state });
        let [MachineEvent::Machines { machines }] = sent.as_slice() else { panic!("not every Machine") };
        assert!(machines[0].is_connected);
        assert_eq!(machines[0].edit_problem, None);
        assert_eq!(machines[0].project_states[0].state, Some(ServiceState::Stopped));

        let mut status = ServiceStatus::never_run(web.id);
        status.state = ServiceState::Failed;
        let event = ServiceEvent::ServiceStateChanged(status);
        let sent = apply(&mut shown, MachinesEvent::ServiceChanged { machine_id: devbox.id, event });
        assert!(
            matches!(&sent[0], MachineEvent::Status { status, .. } if status.url_to_open.as_deref() == Some("http://localhost:3000"))
        );
        assert!(matches!(
            &sent[1],
            MachineEvent::ProjectState { project_state, .. } if project_state.state == Some(ServiceState::Failed)
        ));

        // Renamed: its Projects and statuses stay.
        let renamed = Machine { name: "Dev box".into(), ..devbox.clone() };
        let sent = apply(&mut shown, MachinesEvent::MachinesChanged { machines: vec![renamed] });
        let [MachineEvent::Machines { machines }] = sent.as_slice() else { panic!("not every Machine") };
        assert_eq!((machines[0].machine.name.as_str(), machines[0].projects.len()), ("Dev box", 1));
        assert_eq!(machines[0].statuses[0].status.state, ServiceState::Failed);
    }
}
