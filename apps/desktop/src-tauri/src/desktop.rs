//! Open in Browser, File Manager, Terminal and editor. App-level only: the
//! core knows the folders and the URL, not the apps that open them.
//!
//! On Linux, URLs and folders go to `xdg-open`. The terminal is `$TERMINAL`,
//! else `xdg-terminal-exec`, else the first well-known one installed. The
//! editor is a command the user picks once (no default), run through the
//! login shell like a Service's command (ADR-0003), so it finds what the
//! user's shell finds. macOS (development builds only) uses `open`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use localhost_core::{Id, LocalhostError, RunEnvironment};
use serde::Deserialize;
use specta::Type;
use tokio::process::Command;

use crate::AppState;
use crate::bridge::{CommandError, CommandResult};

/// A folder an action opens.
#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Folder {
    /// The Project's root folder.
    #[serde(rename_all = "camelCase")]
    Project { project_id: Id },
    /// The Service's working directory.
    #[serde(rename_all = "camelCase")]
    Service { service_id: Id },
}

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderApp {
    FileManager,
    Terminal,
    Editor,
}

/// How long an editor command may take to fail before it counts as opened.
/// A missing command makes the shell exit at once; an editor that stays
/// open (or hands off to a running one and exits) is fine.
const EDITOR_CHECK: Duration = Duration::from_secs(3);

/// Opens the Service's URL (see `Service::url_to_open`) in the browser.
#[tauri::command]
#[specta::specta]
pub fn open_service_url(state: tauri::State<'_, AppState>, service_id: Id) -> CommandResult {
    let core = state.core()?;
    let (_, service) = core.library.service(service_id)?;
    let run = core.services.status(service_id).run;
    let Some(url) = service.url_to_open(run.as_ref()) else { return Ok(()) };
    spawn(opener(), &[url.as_str()], None)
}

#[tauri::command]
#[specta::specta]
pub async fn open_folder(state: tauri::State<'_, AppState>, folder: Folder, app: FolderApp) -> CommandResult {
    let core = state.core()?;
    let path = match folder {
        Folder::Project { project_id } => PathBuf::from(core.library.project(project_id)?.path),
        Folder::Service { service_id } => {
            let (project, service) = core.library.service(service_id)?;
            project.working_directory(&service)
        }
    };
    if !path.is_dir() {
        return Err(LocalhostError::WorkingDirectoryMissing(path.display().to_string()).into());
    }
    match app {
        FolderApp::FileManager => open_in_file_manager(&path),
        FolderApp::Terminal => open_terminal(&path),
        FolderApp::Editor => {
            let Some(editor) = state.settings.get().editor_command else {
                return Err(CommandError::new(LocalhostError::CommandNotFound, "Choose an editor in Settings first."));
            };
            open_in_editor(&editor, &path).await
        }
    }
}

pub fn open_in_file_manager(folder: &Path) -> CommandResult {
    spawn(opener(), &[&folder.to_string_lossy()], None)
}

fn opener() -> &'static str {
    if cfg!(target_os = "macos") { "open" } else { "xdg-open" }
}

/// Terminals with the option that opens them in a folder. All of them also
/// start in their own working directory, which `spawn` sets.
const TERMINALS: &[(&str, &[&str])] = &[
    ("xdg-terminal-exec", &[]),
    ("ptyxis", &["--new-window", "--working-directory"]),
    ("gnome-terminal", &["--working-directory"]),
    ("kgx", &["--working-directory"]),
    ("konsole", &["--workdir"]),
    ("xfce4-terminal", &["--working-directory"]),
    ("tilix", &["--working-directory"]),
    ("kitty", &["--directory"]),
    ("alacritty", &["--working-directory"]),
    ("wezterm", &["start", "--cwd"]),
    ("foot", &["--working-directory"]),
    ("x-terminal-emulator", &[]),
    ("xterm", &[]),
];

fn open_terminal(folder: &Path) -> CommandResult {
    if cfg!(target_os = "macos") {
        return spawn("open", &["-a", "Terminal", &folder.to_string_lossy()], None);
    }
    let folder_argument = folder.to_string_lossy();
    if let Some(terminal) = std::env::var("TERMINAL").ok().filter(|terminal| !terminal.trim().is_empty()) {
        let mut words = terminal.split_whitespace();
        let program = words.next().unwrap_or_default();
        return spawn(program, &words.collect::<Vec<_>>(), Some(folder));
    }
    let Some((program, options)) = TERMINALS.iter().find(|(program, _)| is_installed(program)) else {
        return Err(CommandError::new(
            LocalhostError::CommandNotFound,
            "No terminal app was found. Set $TERMINAL to the one you use.",
        ));
    };
    let mut arguments = options.to_vec();
    if !options.is_empty() {
        arguments.push(&folder_argument);
    }
    spawn(program, &arguments, Some(folder))
}

/// Runs `<editor> '<folder>'` through the login shell. Reports a command
/// that isn't found (or can't run) if the shell says so within `EDITOR_CHECK`.
async fn open_in_editor(editor: &str, folder: &Path) -> CommandResult {
    let shell = RunEnvironment::shell(&RunEnvironment::current());
    let line = format!("{editor} {}", shell_quoted(&folder.to_string_lossy()));
    let mut child = Command::new(&shell)
        .args(["-l", "-c", &line])
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| launch_failed(&shell, &error))?;
    match tokio::time::timeout(EDITOR_CHECK, child.wait()).await {
        Ok(Ok(status)) if status.code() == Some(127) => Err(CommandError::new(
            LocalhostError::CommandNotFound,
            format!("The editor command “{editor}” was not found. Check it in Settings."),
        )),
        Ok(Ok(status)) if status.code() == Some(126) => Err(CommandError::new(
            LocalhostError::CommandNotExecutable,
            format!("The editor command “{editor}” could not be run. Check it in Settings."),
        )),
        Ok(_) => Ok(()),
        Err(_) => {
            // Still open: collect its exit later, so it never lingers as a zombie.
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
            });
            Ok(())
        }
    }
}

/// Starts `program` without waiting for it; its exit is collected later.
fn spawn(program: &str, arguments: &[&str], folder: Option<&Path>) -> CommandResult {
    let mut command = std::process::Command::new(program);
    command.args(arguments).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    let mut child = command.spawn().map_err(|error| launch_failed(program, &error))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

fn launch_failed(program: &str, error: &std::io::Error) -> CommandError {
    CommandError::new(
        LocalhostError::LaunchFailed { errno: error.raw_os_error().unwrap_or(0) },
        format!("{program} could not be started: {error}."),
    )
}

fn is_installed(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|folder| folder.join(program).is_file()))
}

/// `text` in single quotes, for POSIX shells and fish alike.
fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_for_the_shell() {
        assert_eq!(shell_quoted("/home/me/my app"), "'/home/me/my app'");
        assert_eq!(shell_quoted("/tmp/it's"), r"'/tmp/it'\''s'");
    }

    #[tokio::test]
    async fn reports_an_editor_command_that_does_not_exist() {
        let error = open_in_editor("localhost-no-such-editor", &std::env::temp_dir()).await.unwrap_err();
        assert!(matches!(error.error, LocalhostError::CommandNotFound));
        assert!(error.message.contains("localhost-no-such-editor"));
    }

    #[tokio::test]
    async fn an_editor_that_exits_or_stays_open_counts_as_opened() {
        assert!(open_in_editor("true", &std::env::temp_dir()).await.is_ok());
    }
}
