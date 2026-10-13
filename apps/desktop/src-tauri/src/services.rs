//! Starting and stopping Services. A failure to start (a port conflict, a
//! missing command) is part of the Service's status, not an error here. A
//! start blocked by a taken port also returns what the UI asks the user.

use localhost_core::{Id, Listener, LocalhostError, Origin, ServiceState, ServiceStatus};
use serde::Serialize;
use specta::Type;

use crate::bridge::CommandResult;
use crate::listeners::ListenerDetails;
use crate::{AppState, Core};

/// A start that Failed because its port is taken, waiting for the user to
/// stop the holder, start on another port, or cancel.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BlockedStart {
    service_id: Id,
    port: u16,
    /// Who holds the port; `None` if localhost can't see it.
    holder: Option<ListenerDetails>,
    /// The port a start could use instead; `None` if none within 100 above is free.
    next_free_port: Option<u16>,
    /// The names of the Services that depend on it, which may still expect its Port.
    dependents: Vec<String>,
}

/// Starts the Service, on `port` instead of its Port if given (for this Run
/// only). Returns the question to ask if its port is taken.
#[tauri::command]
#[specta::specta]
pub async fn start_service(
    state: tauri::State<'_, AppState>,
    service_id: Id,
    port: Option<u16>,
) -> CommandResult<Option<BlockedStart>> {
    let core = state.core()?;
    let status = core.services.start(service_id, port, Origin::MainWindow).await?;
    blocked_start(core, &status).await
}

#[tauri::command]
#[specta::specta]
pub async fn stop_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    state.core()?.services.stop(service_id, Origin::MainWindow).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn restart_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult<Option<BlockedStart>> {
    let core = state.core()?;
    let status = core.services.restart(service_id, Origin::MainWindow).await?;
    blocked_start(core, &status).await
}

/// "Stop it and start": stops whatever holds the port, then starts the Service.
#[tauri::command]
#[specta::specta]
pub async fn stop_holder_and_start(
    state: tauri::State<'_, AppState>,
    service_id: Id,
    holder: Listener,
) -> CommandResult<Option<BlockedStart>> {
    let core = state.core()?;
    core.services.stop_listener(&holder, Origin::MainWindow).await?;
    let status = core.services.start(service_id, None, Origin::MainWindow).await?;
    blocked_start(core, &status).await
}

/// Starts the Project's Services, Dependencies first. A blocked Service
/// Fails with its port conflict; nothing is asked.
#[tauri::command]
#[specta::specta]
pub async fn start_all(state: tauri::State<'_, AppState>, project_id: Id) -> CommandResult {
    Ok(state.core()?.services.start_all(project_id, Origin::MainWindow).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn stop_all(state: tauri::State<'_, AppState>, project_id: Id) -> CommandResult {
    Ok(state.core()?.services.stop_all(project_id, Origin::MainWindow).await?)
}

async fn blocked_start(core: &Core, status: &ServiceStatus) -> CommandResult<Option<BlockedStart>> {
    let failure = status.run.as_ref().and_then(|run| run.failure.clone());
    let (ServiceState::Failed, Some(LocalhostError::PortConflict { port, owner })) = (status.state, failure) else {
        return Ok(None);
    };
    let service_id = status.service_id;
    let next_free_port = core.services.next_free_port(service_id).await?;
    let (project, _) = core.library.service(service_id)?;
    let projects = core.library.projects();
    Ok(Some(BlockedStart {
        service_id,
        port,
        holder: owner.map(|owner| ListenerDetails::new(*owner, &projects)),
        next_free_port,
        dependents: project.dependents(service_id).iter().map(|service| service.name.clone()).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use localhost_core::{Key, NewService, ServiceChanges};

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_start_blocked_by_a_taken_port_says_who_holds_it_and_where_else_to_start() {
        let root = std::env::temp_dir().join(format!("localhost-blocked-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let core = Core::open(&root).unwrap();
        // This test process holds the port, so it is a holder localhost won't stop.
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        let web = NewService {
            port: Some(port.into()),
            port_can_change: false,
            ..NewService::new("Web", Key::new("web").unwrap(), "sleep 30")
        };
        let worker = NewService::new("Worker", Key::new("worker").unwrap(), "sleep 30");
        let project = core
            .library
            .add_project("Shop", Key::new("shop").unwrap(), &root.to_string_lossy(), vec![web, worker], Origin::Cli)
            .unwrap();
        let (web, worker) = (&project.services[0], &project.services[1]);
        let changes = ServiceChanges { dependencies: Some(vec![web.id]), ..Default::default() };
        core.library.edit_service(worker.id, changes, Origin::Cli).unwrap();

        let status = core.services.start(web.id, None, Origin::MainWindow).await.unwrap();
        let blocked = blocked_start(&core, &status).await.unwrap().expect("the start is blocked");

        assert_eq!(blocked.port, port);
        let next = blocked.next_free_port.expect("a free port above it");
        assert!(next > port && next <= port + 100);
        assert_eq!(blocked.dependents, ["Worker"]);
        let holder = blocked.holder.expect("this process holds it");
        assert_eq!(holder.listener.pid, std::process::id() as i32);
        assert_eq!(holder.reason_not_stoppable, Some(LocalhostError::ListenerIsLocalhost.message()));

        let started = core.services.start(web.id, Some(next), Origin::MainWindow).await.unwrap();
        assert!(blocked_start(&core, &started).await.unwrap().is_none());
        core.services.stop_every_run(Origin::MainWindow).await;
        let _ = std::fs::remove_dir_all(root);
    }
}
