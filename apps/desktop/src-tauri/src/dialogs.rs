//! The system's file pickers, for the editors: a Project's folder, a
//! Service's working directory, and the Project files of Import and Export.
//! Each returns `None` when the user cancels.
//!
//! On Windows a WSL Project's folder is picked where Windows sees it,
//! `\\wsl.localhost\<distro>\…`, and kept as the distro's own path (`wsl`).

use std::path::{Component, Path, PathBuf};

use localhost_core::{ExecutionTarget, Id};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::WebviewWindow;
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};
use tokio::sync::oneshot;

use crate::bridge::CommandResult;
use crate::{AppState, wsl};

const PROJECT_FILE_TYPES: &[&str] = &["yaml", "yml"];

/// A Project's folder: the Execution Target it runs on, and its path there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFolder {
    pub target: ExecutionTarget,
    pub path: String,
}

impl ProjectFolder {
    /// The folder the picker returned, on the target it belongs to. A
    /// distro's own folder runs in that distro. A Windows drive's folder
    /// runs in `target`'s distro, through the distro's mount of the drive,
    /// or on this machine.
    fn picked(picked: &str, target: &ExecutionTarget) -> ProjectFolder {
        if let Some((distro, path)) = wsl::linux_path(picked) {
            return ProjectFolder { target: ExecutionTarget::Wsl { distro }, path };
        }
        if let (ExecutionTarget::Wsl { .. }, Some(path)) = (target, wsl::mounted_path(picked)) {
            return ProjectFolder { target: target.clone(), path };
        }
        ProjectFolder { target: ExecutionTarget::ThisMachine, path: picked.to_string() }
    }

    /// Whether this app can see the folder, e.g. to open it.
    pub fn exists(&self) -> bool {
        self.local_path().is_some_and(|path| path.is_dir())
    }

    /// Where this app sees it (see `wsl::local_path`).
    pub fn local_path(&self) -> Option<PathBuf> {
        wsl::local_path(&self.target, &self.path)
    }
}

/// A Project's folder, starting in `start` (a path on `target`) when it
/// exists, or in a distro's `/home`.
#[tauri::command]
#[specta::specta]
pub async fn choose_folder(
    window: WebviewWindow,
    target: ExecutionTarget,
    start: Option<String>,
) -> Option<ProjectFolder> {
    let mut dialog = picker(&window).set_title("Choose a Folder");
    let home = (!target.is_this_machine()).then(|| "/home".to_string());
    let start = [start, home].into_iter().flatten().find_map(|start| {
        ProjectFolder { target: target.clone(), path: start }.local_path().filter(|path| path.is_dir())
    });
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    let folder = pick(|send| dialog.pick_folder(send)).await?;
    Some(ProjectFolder::picked(&folder.to_string_lossy(), &target))
}

/// A folder for a Service's working directory, relative to the Project's
/// folder, e.g. `web` or `../shared`. `None` if it isn't where the Project runs.
#[tauri::command]
#[specta::specta]
pub async fn choose_working_directory(window: WebviewWindow, project: ProjectFolder) -> Option<String> {
    let mut dialog = picker(&window).set_title("Choose the Working Directory");
    if let Some(start) = project.local_path() {
        dialog = dialog.set_directory(start);
    }
    let folder = pick(|send| dialog.pick_folder(send)).await?;
    let picked = ProjectFolder::picked(&folder.to_string_lossy(), &project.target);
    (picked.target == project.target).then(|| relative_path(Path::new(&picked.path), Path::new(&project.path)))
}

/// A Project file to import.
#[tauri::command]
#[specta::specta]
pub async fn choose_project_file(window: WebviewWindow) -> Option<String> {
    let dialog = picker(&window).set_title("Import Project").add_filter("Project file (YAML)", PROJECT_FILE_TYPES);
    let file = pick(|send| dialog.pick_file(send)).await?;
    Some(file.to_string_lossy().into_owned())
}

/// Where to save the Project's file, named after its Key.
#[tauri::command]
#[specta::specta]
pub async fn choose_export_file(
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
    project_id: Id,
) -> CommandResult<Option<String>> {
    let project = state.core()?.library.project(project_id)?;
    let dialog = picker(&window)
        .set_title(format!("Export {}", project.name))
        .set_file_name(format!("{}.yaml", project.key))
        .add_filter("Project file (YAML)", PROJECT_FILE_TYPES);
    let file = pick(|send| dialog.save_file(send)).await;
    Ok(file.map(|file| file.to_string_lossy().into_owned()))
}

fn picker(window: &WebviewWindow) -> FileDialogBuilder<tauri::Wry> {
    window.dialog().file().set_parent(window)
}

/// Opens the dialog, which calls back once it closes, and waits for its answer.
async fn pick(open: impl FnOnce(Box<dyn FnOnce(Option<FilePath>) + Send>)) -> Option<PathBuf> {
    let (sender, receiver) = oneshot::channel();
    open(Box::new(move |path| {
        let _ = sender.send(path);
    }));
    receiver.await.ok().flatten()?.into_path().ok()
}

/// `path` relative to `base`, e.g. `web` or `../shared`; `.` for `base` itself.
fn relative_path(path: &Path, base: &Path) -> String {
    let components = |path: &Path| -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    parts.pop();
                }
                _ => {}
            }
        }
        parts
    };
    let (target, root) = (components(path), components(base));
    let common = target.iter().zip(&root).take_while(|(a, b)| a == b).count();
    let parts: Vec<String> =
        std::iter::repeat_n("..".to_string(), root.len() - common).chain(target[common..].iter().cloned()).collect();
    if parts.is_empty() { ".".into() } else { parts.join("/") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picked_folder_runs_where_it_is() {
        let ubuntu = ExecutionTarget::Wsl { distro: "Ubuntu".into() };
        let folder = |target: &ExecutionTarget, path: &str| ProjectFolder { target: target.clone(), path: path.into() };
        let this_machine = ExecutionTarget::ThisMachine;
        assert_eq!(
            ProjectFolder::picked(r"\\wsl.localhost\Ubuntu\home\me\shop", &this_machine),
            folder(&ubuntu, "/home/me/shop")
        );
        assert_eq!(ProjectFolder::picked(r"C:\Users\me\shop", &ubuntu), folder(&ubuntu, "/mnt/c/Users/me/shop"));
        assert_eq!(
            ProjectFolder::picked(r"C:\Users\me\shop", &this_machine),
            folder(&this_machine, r"C:\Users\me\shop")
        );
        assert_eq!(ProjectFolder::picked("/home/me/shop", &ubuntu), folder(&this_machine, "/home/me/shop"));
    }

    #[test]
    fn makes_folders_relative_to_the_project() {
        let project = Path::new("/home/me/shop");
        assert_eq!(relative_path(Path::new("/home/me/shop"), project), ".");
        assert_eq!(relative_path(Path::new("/home/me/shop/apps/web"), project), "apps/web");
        assert_eq!(relative_path(Path::new("/home/me/shared"), project), "../shared");
        assert_eq!(relative_path(Path::new("/home/me/shop/./web/"), Path::new("/home/me/shop/")), "web");
    }
}
