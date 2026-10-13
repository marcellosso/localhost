//! Starting and stopping Services. A failure to start (a port conflict, a
//! missing command) is part of the Service's status, not an error here.

use localhost_core::{Id, Origin};

use crate::AppState;
use crate::bridge::CommandResult;

#[tauri::command]
#[specta::specta]
pub async fn start_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    state.core()?.services.start(service_id, None, Origin::MainWindow).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn stop_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    state.core()?.services.stop(service_id, Origin::MainWindow).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn restart_service(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    state.core()?.services.restart(service_id, Origin::MainWindow).await?;
    Ok(())
}

/// Starts the Project's Services, Dependencies first.
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
