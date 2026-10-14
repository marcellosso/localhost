//! The system's file pickers, for the editors: a Project's folder, a
//! Service's working directory, and the Project files of Import and Export.
//! Each returns `None` when the user cancels.

use std::path::{Component, Path, PathBuf};

use localhost_core::Id;
use tauri::WebviewWindow;
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};
use tokio::sync::oneshot;

use crate::AppState;
use crate::bridge::CommandResult;

const PROJECT_FILE_TYPES: &[&str] = &["yaml", "yml"];

/// A folder, starting in `start` when it exists.
#[tauri::command]
#[specta::specta]
pub async fn choose_folder(window: WebviewWindow, start: Option<String>) -> Option<String> {
    let mut dialog = picker(&window).set_title("Choose a Folder");
    if let Some(start) = start.filter(|start| Path::new(start).is_dir()) {
        dialog = dialog.set_directory(start);
    }
    let folder = pick(|send| dialog.pick_folder(send)).await?;
    Some(folder.to_string_lossy().into_owned())
}

/// A folder for a Service's working directory, relative to `project_path`,
/// e.g. `web` or `../shared`.
#[tauri::command]
#[specta::specta]
pub async fn choose_working_directory(window: WebviewWindow, project_path: String) -> Option<String> {
    let dialog = picker(&window).set_title("Choose the Working Directory").set_directory(&project_path);
    let folder = pick(|send| dialog.pick_folder(send)).await?;
    Some(relative_path(&folder, Path::new(&project_path)))
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
    fn makes_folders_relative_to_the_project() {
        let project = Path::new("/home/me/shop");
        assert_eq!(relative_path(Path::new("/home/me/shop"), project), ".");
        assert_eq!(relative_path(Path::new("/home/me/shop/apps/web"), project), "apps/web");
        assert_eq!(relative_path(Path::new("/home/me/shared"), project), "../shared");
        assert_eq!(relative_path(Path::new("/home/me/shop/./web/"), Path::new("/home/me/shop/")), "web");
    }
}
