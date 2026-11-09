//! Anonymous usage telemetry: the core decides what is sent and when
//! (`localhost_core::Telemetry`); this exposes the user's choice and the
//! first-open notice to the UI.

use crate::AppState;
use crate::bridge::CommandResult;

/// Whether the user lets localhost send anonymous usage data (Settings › General).
#[tauri::command]
#[specta::specta]
pub fn telemetry_enabled(state: tauri::State<'_, AppState>) -> bool {
    state.telemetry.is_enabled()
}

#[tauri::command]
#[specta::specta]
pub fn set_telemetry_enabled(state: tauri::State<'_, AppState>, enabled: bool) -> CommandResult {
    Ok(state.telemetry.set_enabled(enabled)?)
}

/// Whether the sidebar should say, once, what is sent and how to turn it off.
#[tauri::command]
#[specta::specta]
pub fn telemetry_notice_pending(state: tauri::State<'_, AppState>) -> bool {
    state.telemetry.is_notice_pending()
}

#[tauri::command]
#[specta::specta]
pub fn acknowledge_telemetry_notice(state: tauri::State<'_, AppState>) {
    state.telemetry.acknowledge_notice();
}
