//! Listeners for the Ports view and the port-conflict question. Who owns a
//! Listener and whether it may be stopped are decided by the core; the UI
//! only shows the result.

use localhost_core::{Listener, Origin, Project};
use serde::Serialize;
use specta::Type;

use crate::AppState;
use crate::bridge::CommandResult;

/// A Listener with what the UI says about it.
#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListenerDetails {
    pub(crate) listener: Listener,
    /// Who it belongs to, e.g. "My SaaS › API", "In My SaaS", "Docker", "Unmanaged".
    owner: String,
    /// Why localhost won't stop it; `None` if it can.
    pub(crate) reason_not_stoppable: Option<String>,
}

impl ListenerDetails {
    pub fn new(listener: Listener, projects: &[Project]) -> ListenerDetails {
        ListenerDetails {
            owner: listener.owner_description(projects),
            reason_not_stoppable: listener.reason_not_stoppable().map(|reason| reason.message()),
            listener,
        }
    }
}

/// Every Listener on this machine, scanned now. The Ports view polls this
/// while it is visible.
#[tauri::command]
#[specta::specta]
pub async fn listeners(state: tauri::State<'_, AppState>) -> CommandResult<Vec<ListenerDetails>> {
    let core = state.core()?;
    let projects = core.library.projects();
    let listeners = core.services.listeners().await;
    Ok(listeners.into_iter().map(|listener| ListenerDetails::new(listener, &projects)).collect())
}

/// Stops a Listener: its Service for a Run's, otherwise its processes (the
/// UI confirms first). Returns once it has ended.
#[tauri::command]
#[specta::specta]
pub async fn stop_listener(state: tauri::State<'_, AppState>, listener: Listener) -> CommandResult {
    Ok(state.core()?.services.stop_listener(&listener, Origin::MainWindow).await?)
}
