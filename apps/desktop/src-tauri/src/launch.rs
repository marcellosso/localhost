//! What the user must hear about at launch: Project files that couldn't be
//! loaded, then Orphans of an earlier session.
//! This machine's Orphans are looked for once, before anything is started;
//! a WSL distro's when it is first connected, which `OrphansFound` tells
//! the window. Both questions stay pending until answered, so reloading
//! the window asks again.

use std::path::Path;
use std::sync::Mutex;

use localhost_core::{ExecutionTarget, Orphan};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::AppState;
use crate::bridge::CommandResult;
use crate::desktop;

pub struct Launch {
    is_unreadable_files_reported: bool,
    orphans: Vec<(ExecutionTarget, Orphan)>,
}

impl Launch {
    /// `orphans` are this machine's.
    pub fn new(orphans: Vec<Orphan>) -> Mutex<Launch> {
        let orphans = orphans.into_iter().map(|orphan| (ExecutionTarget::ThisMachine, orphan)).collect();
        Mutex::new(Launch { is_unreadable_files_reported: false, orphans })
    }
}

/// A WSL distro had Orphans as it connected: the window asks about them.
#[derive(Clone, Debug, Serialize, Type, Event)]
pub struct OrphansFound;

/// Adds the Orphans each target finds as it connects to the launch question.
pub fn watch_found_orphans(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(core) = state.core() else { return };
        let mut found = core.targets.found_orphans();
        while let Some(found) = found.recv().await {
            let target = found.target;
            let orphans = found.orphans.into_iter().map(|orphan| (target.clone(), orphan));
            state.launch.lock().unwrap().orphans.extend(orphans);
            let _ = OrphansFound.emit(&app);
        }
    });
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
    let orphans = launch.orphans.iter().map(|(_, orphan)| orphan.description(&projects)).collect();
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
    for (target, orphans) in by_target(orphans) {
        core.targets.stop_orphans(&target, &orphans).await?;
    }
    Ok(())
}

/// Forgets the Orphans, leaving their processes running.
#[tauri::command]
#[specta::specta]
pub async fn leave_orphans(state: tauri::State<'_, AppState>) -> CommandResult {
    let core = state.core()?;
    let orphans = std::mem::take(&mut state.launch.lock().unwrap().orphans);
    for (target, orphans) in by_target(orphans) {
        core.targets.leave_orphans(&target, &orphans).await?;
    }
    Ok(())
}

fn by_target(orphans: Vec<(ExecutionTarget, Orphan)>) -> Vec<(ExecutionTarget, Vec<Orphan>)> {
    let mut grouped: Vec<(ExecutionTarget, Vec<Orphan>)> = Vec::new();
    for (target, orphan) in orphans {
        match grouped.iter_mut().find(|(existing, _)| *existing == target) {
            Some((_, group)) => group.push(orphan),
            None => grouped.push((target, vec![orphan])),
        }
    }
    grouped
}
