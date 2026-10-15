//! Open in Browser, File Manager, Terminal and editor. App-level only: the
//! core knows the folders and the URL, not the apps that open them.
//!
//! On Linux, URLs and folders go to `xdg-open`. The terminal is `$TERMINAL`,
//! else `xdg-terminal-exec`, else the first well-known one installed. The
//! editor is a command the user picks once (no default), run through the
//! login shell like a Service's command (ADR-0003), so it finds what the
//! user's shell finds. macOS (development builds only) uses `open`.
//!
//! On Windows, URLs and folders go to Explorer, and the terminal is Windows
//! Terminal (a console window without it). A WSL Project's folder opens in
//! a terminal of its distro (its Windows Terminal profile), in Explorer at
//! `\\wsl.localhost\…`, and in VS Code and its kin through their WSL
//! remote (`code --remote wsl+<distro>`). The editor command runs through
//! `cmd.exe` there.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use localhost_core::{ExecutionTarget, Id, LocalhostError, RunEnvironment};
use serde::Deserialize;
use specta::Type;
#[cfg(not(windows))]
use tokio::process::Command;

use crate::bridge::{CommandError, CommandResult};
use crate::dialogs::ProjectFolder;
use crate::{AppState, Core, wsl};

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
    open_url_of(state.core()?, service_id)
}

/// Open in Browser, for the window and the tray alike.
pub fn open_url_of(core: &Core, service_id: Id) -> CommandResult {
    let (_, service) = core.library.service(service_id)?;
    let run = core.targets.status(service_id).run;
    let Some(url) = service.url_to_open(run.as_ref()) else { return Ok(()) };
    spawn(opener(), &[url.as_str()], None)
}

/// Opens a URL printed in a log. Only web addresses: anything else could
/// open a file or an app.
#[tauri::command]
#[specta::specta]
pub fn open_url(url: String) -> CommandResult {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(LocalhostError::InvalidUrl(url).into());
    }
    spawn(opener(), &[url.as_str()], None)
}

#[tauri::command]
#[specta::specta]
pub async fn open_folder(state: tauri::State<'_, AppState>, folder: Folder, app: FolderApp) -> CommandResult {
    let core = state.core()?;
    let folder = match folder {
        Folder::Project { project_id } => {
            let project = core.library.project(project_id)?;
            ProjectFolder { target: project.execution_target, path: project.path }
        }
        Folder::Service { service_id } => {
            let (project, service) = core.library.service(service_id)?;
            let path = match &project.execution_target {
                ExecutionTarget::ThisMachine => project.working_directory(&service).to_string_lossy().into_owned(),
                ExecutionTarget::Wsl { .. } => wsl::joined(&project.path, &service.working_directory),
            };
            ProjectFolder { target: project.execution_target, path }
        }
    };
    let Some(path) = folder.local_path() else {
        let ExecutionTarget::Wsl { distro } = &folder.target else { unreachable!("this machine's folders are local") };
        return Err(
            LocalhostError::TargetUnavailable(format!("“{distro}” is reached only from the Windows app")).into()
        );
    };
    if !path.is_dir() {
        return Err(LocalhostError::WorkingDirectoryMissing(folder.path).into());
    }
    match app {
        FolderApp::FileManager => open_in_file_manager(&path),
        FolderApp::Terminal => open_terminal(&folder, &path),
        FolderApp::Editor => {
            let Some(editor) = state.settings.get().editor_command else {
                return Err(CommandError::new(LocalhostError::CommandNotFound, "Choose an editor in Settings first."));
            };
            open_in_editor(&editor, &editor_arguments(&editor, &folder, &path), &path).await
        }
    }
}

pub fn open_in_file_manager(folder: &Path) -> CommandResult {
    spawn(opener(), &[&folder.to_string_lossy()], None)
}

fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer.exe"
    } else {
        "xdg-open"
    }
}

/// Terminals with the option that opens them in a folder. All of them also
/// start in their own working directory, which `spawn` sets.
#[cfg(not(windows))]
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

#[cfg(windows)]
fn open_terminal(folder: &ProjectFolder, path: &Path) -> CommandResult {
    let has_windows_terminal = is_installed("wt.exe");
    let (arguments, folder_to_start_in): (Vec<&str>, _) = match &folder.target {
        ExecutionTarget::Wsl { distro } => {
            if has_windows_terminal {
                // Windows Terminal reads `;` as the start of another tab.
                let escaped = folder.path.replace(';', r"\;");
                let shell = ["wsl.exe", "--distribution", distro, "--cd", &escaped];
                return spawn("wt.exe", &[&["new-tab", "--profile", distro], &shell[..]].concat(), None);
            }
            // `cmd.exe` can't start in a `\\wsl.localhost` folder; `--cd` gets there.
            (vec!["wsl.exe", "--distribution", distro, "--cd", &folder.path], None)
        }
        ExecutionTarget::ThisMachine => {
            if has_windows_terminal {
                return spawn("wt.exe", &["new-tab", "--startingDirectory", &path.to_string_lossy()], None);
            }
            (vec!["cmd.exe"], Some(path))
        }
    };
    // No Windows Terminal: `start` gives the shell a console window of its own.
    windows::spawn_through_cmd(&format!("start \"\" {}", windows::cmd_quoted(&arguments)), folder_to_start_in)
}

#[cfg(not(windows))]
fn open_terminal(folder: &ProjectFolder, _: &Path) -> CommandResult {
    let folder = Path::new(&folder.path);
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

/// Editors that open a distro's folder through their WSL remote; others
/// get the folder as Windows sees it.
const WSL_REMOTE_EDITORS: &[&str] = &["code", "code-insiders", "codium", "cursor", "windsurf"];

/// What follows the editor command: the folder, or for a WSL Project in a
/// VS Code kin, `--remote wsl+<distro> <path in the distro>`.
fn editor_arguments(editor: &str, folder: &ProjectFolder, path: &Path) -> Vec<String> {
    let program = editor.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
    let name = name.trim_end_matches(".cmd").trim_end_matches(".exe");
    match &folder.target {
        ExecutionTarget::Wsl { distro } if WSL_REMOTE_EDITORS.contains(&name) => {
            vec!["--remote".into(), format!("wsl+{distro}"), folder.path.clone()]
        }
        _ => vec![path.to_string_lossy().into_owned()],
    }
}

/// Runs `<editor> <arguments>` through the login shell (`cmd.exe` on
/// Windows). Reports a command that isn't found (or can't run) if the shell
/// says so within `EDITOR_CHECK`.
async fn open_in_editor(editor: &str, arguments: &[String], folder: &Path) -> CommandResult {
    let environment = RunEnvironment::current();
    #[cfg(windows)]
    let (shell, mut command) = {
        let quoted: Vec<&str> = arguments.iter().map(String::as_str).collect();
        windows::cmd(&format!("{editor} {}", windows::cmd_quoted(&quoted)))
    };
    #[cfg(not(windows))]
    let (shell, mut command) = {
        let shell = RunEnvironment::shell(&environment);
        let quoted: Vec<String> = arguments.iter().map(|argument| shell_quoted(argument)).collect();
        let mut command = Command::new(&shell);
        command.args(["-l", "-c", &format!("{editor} {}", quoted.join(" "))]);
        (shell, command)
    };
    let mut child = command
        .env_clear()
        .envs(environment)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| launch_failed(&shell, &error))?;
    // `cmd.exe` exits with 9009 for a command it doesn't know.
    let not_found = if cfg!(windows) { 9009 } else { 127 };
    match tokio::time::timeout(EDITOR_CHECK, child.wait()).await {
        Ok(Ok(status)) if status.code() == Some(not_found) => Err(CommandError::new(
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
/// It gets the environment a Service would, without an AppImage's paths.
fn spawn(program: &str, arguments: &[&str], folder: Option<&Path>) -> CommandResult {
    let mut command = std::process::Command::new(program);
    command.args(arguments).env_clear().envs(RunEnvironment::current());
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
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
    // Windows Terminal's `wt.exe` is an app alias, which `is_file` can't follow.
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|folder| std::fs::symlink_metadata(folder.join(program)).is_ok())
    })
}

/// `text` in single quotes, for POSIX shells and fish alike.
#[cfg(not(windows))]
fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(windows)]
mod windows {
    use std::path::Path;
    use std::process::Stdio;

    use localhost_core::RunEnvironment;
    use tokio::process::Command;

    use super::launch_failed;
    use crate::bridge::CommandResult;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// `cmd.exe /c <line>`, without a window of its own, and its name.
    pub(super) fn cmd(line: &str) -> (String, Command) {
        let mut command = Command::new("cmd.exe");
        // `/s`: the outer quotes are removed, and the line kept as it is.
        command.raw_arg(format!("/d /s /c \"{line}\"")).creation_flags(CREATE_NO_WINDOW);
        ("cmd.exe".into(), command)
    }

    /// Runs `line` through `cmd.exe`, in `folder` if any, without waiting for it.
    pub(super) fn spawn_through_cmd(line: &str, folder: Option<&Path>) -> CommandResult {
        let (shell, mut command) = cmd(line);
        command.env_clear().envs(RunEnvironment::current());
        if let Some(folder) = folder {
            command.current_dir(folder);
        }
        command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = command.spawn().map_err(|error| launch_failed(&shell, &error))?;
        tauri::async_runtime::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(())
    }

    /// The arguments in double quotes, for `cmd.exe`.
    pub(super) fn cmd_quoted(arguments: &[&str]) -> String {
        arguments.iter().map(|argument| format!("\"{}\"", argument.replace('"', "\"\""))).collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_wsl_folder_in_vs_code_through_its_remote() {
        let ubuntu =
            ProjectFolder { target: ExecutionTarget::Wsl { distro: "Ubuntu".into() }, path: "/home/me/shop".into() };
        let seen = Path::new(r"\\wsl.localhost\Ubuntu\home\me\shop");
        let remote = ["--remote", "wsl+Ubuntu", "/home/me/shop"];
        assert_eq!(editor_arguments("code", &ubuntu, seen), remote);
        assert_eq!(editor_arguments("code.cmd --new-window", &ubuntu, seen), remote);
        assert_eq!(editor_arguments(r"C:\Tools\Cursor.exe", &ubuntu, seen), remote);
        assert_eq!(editor_arguments("zed", &ubuntu, seen), [r"\\wsl.localhost\Ubuntu\home\me\shop"]);
        let here = ProjectFolder { target: ExecutionTarget::ThisMachine, path: "/home/me/shop".into() };
        assert_eq!(editor_arguments("code", &here, Path::new("/home/me/shop")), ["/home/me/shop"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn quotes_paths_for_the_shell() {
        assert_eq!(shell_quoted("/home/me/my app"), "'/home/me/my app'");
        assert_eq!(shell_quoted("/tmp/it's"), r"'/tmp/it'\''s'");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn reports_an_editor_command_that_does_not_exist() {
        let folder = std::env::temp_dir();
        let error = open_in_editor("localhost-no-such-editor", &[], &folder).await.unwrap_err();
        assert!(matches!(error.error, LocalhostError::CommandNotFound));
        assert!(error.message.contains("localhost-no-such-editor"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn an_editor_that_exits_or_stays_open_counts_as_opened() {
        assert!(open_in_editor("true", &[], &std::env::temp_dir()).await.is_ok());
    }
}
