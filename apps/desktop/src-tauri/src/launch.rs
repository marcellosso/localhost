//! What the user must hear about at launch: Project files that couldn't be
//! loaded, then Orphans of an earlier session (`docs/v1-decisions.md`).
//! Orphans are looked for once, before anything is started; both questions
//! stay pending until answered, so reloading the window asks again.

use std::path::Path;
use std::sync::Mutex;

use localhost_core::{Orphan, StopPolicy};
use serde::Serialize;
use specta::Type;

use crate::AppState;
use crate::bridge::CommandResult;
use crate::desktop;

pub struct Launch {
    is_unreadable_files_reported: bool,
    orphans: Vec<Orphan>,
}

impl Launch {
    pub fn new(orphans: Vec<Orphan>) -> Mutex<Launch> {
        Mutex::new(Launch { is_unreadable_files_reported: false, orphans })
    }
}

/// The launch questions still to ask, in order.
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LaunchChecks {
    /// One line per Project file that couldn't be loaded: its name, why, and
    /// where it is now.
    unreadable_files: Vec<String>,
    /// One line per Orphan, e.g. "My SaaS › API".
    orphans: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub fn launch_checks(state: tauri::State<'_, AppState>) -> CommandResult<LaunchChecks> {
    let core = state.core()?;
    let launch = state.launch.lock().unwrap();
    let unreadable_files = if launch.is_unreadable_files_reported {
        Vec::new()
    } else {
        core.library.unreadable_files().iter().map(|file| file.description()).collect()
    };
    let projects = core.library.projects();
    let orphans = launch.orphans.iter().map(|orphan| orphan.description(&projects)).collect();
    Ok(LaunchChecks { unreadable_files, orphans })
}

/// Closes the unreadable files report, optionally showing the folder that
/// holds them.
#[tauri::command]
#[specta::specta]
pub fn dismiss_unreadable_files(state: tauri::State<'_, AppState>, reveal: bool) -> CommandResult {
    let core = state.core()?;
    state.launch.lock().unwrap().is_unreadable_files_reported = true;
    let file = core.library.unreadable_files().first();
    if let (true, Some(file)) = (reveal, file) {
        let path = file.moved_to.as_deref().unwrap_or(&file.path);
        if let Some(folder) = Path::new(path).parent() {
            desktop::open_in_file_manager(folder)?;
        }
    }
    Ok(())
}

/// Stops the Orphans' process groups and returns once they have ended.
#[tauri::command]
#[specta::specta]
pub async fn stop_orphans(state: tauri::State<'_, AppState>) -> CommandResult {
    let core = state.core()?;
    let orphans = std::mem::take(&mut state.launch.lock().unwrap().orphans);
    core.runs.stop_orphans(&orphans, StopPolicy::default()).await;
    Ok(())
}

/// Forgets the Orphans, leaving their processes running.
#[tauri::command]
#[specta::specta]
pub fn leave_orphans(state: tauri::State<'_, AppState>) -> CommandResult {
    let core = state.core()?;
    let orphans = std::mem::take(&mut state.launch.lock().unwrap().orphans);
    core.runs.leave_orphans(&orphans);
    Ok(())
}
