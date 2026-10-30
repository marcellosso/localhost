//! Connecting coding agents to the `localhost` command this app ships, for
//! the Connect your agents screen (`localhost_core::AgentSetup`). The
//! command is installed beside `localhost-app`: in the Windows install
//! folder, and in `/usr/bin` by the .deb and .rpm. An AppImage's files live
//! in a mount that goes away when the app quits, so there it is copied to
//! localhost's data folder first, and copied again when an update changes it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use localhost_core::{AgentClient, AgentSetup, AgentStatus, CommandLineStatus};

use crate::bridge::CommandResult;

const COMMAND: &str = if cfg!(windows) { "localhost.exe" } else { "localhost" };

/// At launch, an AppImage's copy of the command is brought up to date, so
/// connected agents run an updated app's version from their next session.
pub fn prepare() {
    if std::env::var_os("APPIMAGE").is_some() {
        std::thread::spawn(|| {
            setup();
        });
    }
}

fn setup() -> &'static AgentSetup {
    static SETUP: OnceLock<AgentSetup> = OnceLock::new();
    SETUP.get_or_init(|| {
        let root = crate::storage_root();
        AgentSetup::new(bundled_command(&root), &root)
    })
}

/// The command agents are pointed at; a path that doesn't exist when this
/// build has none (a development build), which the core reports.
fn bundled_command(storage_root: &Path) -> PathBuf {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(|folder| folder.join(COMMAND)))
        .unwrap_or_else(|| PathBuf::from(COMMAND));
    if std::env::var_os("APPIMAGE").is_none() || !beside.is_file() {
        return beside;
    }
    let copy = storage_root.join("bin").join(COMMAND);
    match copy_if_changed(&beside, &copy) {
        Ok(()) => copy,
        Err(error) => {
            eprintln!("localhost: couldn't copy the localhost command to {}: {error}", copy.display());
            beside
        }
    }
}

/// Replaces `destination` with `source` when they differ, in one step, so an
/// agent running the old copy keeps it until it restarts.
fn copy_if_changed(source: &Path, destination: &Path) -> std::io::Result<()> {
    let contents = fs::read(source)?;
    if fs::read(destination).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    let folder = destination.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(folder)?;
    let temporary = folder.join(format!(".{COMMAND}.{}", std::process::id()));
    fs::write(&temporary, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
    }
    fs::rename(&temporary, destination)
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tauri::async_runtime::spawn_blocking(work).await.expect("the agents' work doesn't panic")
}

/// Whether Connect your agents should open by itself at this launch: once,
/// the first time localhost runs with it.
#[tauri::command]
#[specta::specta]
pub fn take_connect_agents_offer(state: tauri::State<'_, crate::AppState>) -> bool {
    state.settings.take_agents_offer()
}

/// Each agent and where it stands.
#[tauri::command]
#[specta::specta]
pub async fn agent_statuses() -> Vec<AgentStatus> {
    blocking(|| setup().statuses()).await
}

/// Adds or updates the `localhost` server in the agent's configuration, and its skill.
#[tauri::command]
#[specta::specta]
pub async fn connect_agent(client: AgentClient) -> CommandResult<AgentStatus> {
    Ok(blocking(move || setup().connect(client)).await?)
}

/// Removes localhost's entries from the agent's configuration.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_agent(client: AgentClient) -> CommandResult<AgentStatus> {
    Ok(blocking(move || setup().disconnect(client)).await?)
}

/// Whether terminals find the `localhost` command.
#[tauri::command]
#[specta::specta]
pub async fn command_line_status() -> CommandLineStatus {
    blocking(|| setup().command_line()).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_command_to_path() -> CommandResult<CommandLineStatus> {
    Ok(blocking(|| setup().add_command_to_path()).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_command_from_path() -> CommandResult<CommandLineStatus> {
    Ok(blocking(|| setup().remove_command_from_path()).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_appimage_copy_is_replaced_only_when_it_changed() {
        let folder = tempfile_folder();
        let source = folder.join("mount/localhost");
        let copy = folder.join("data/bin/localhost");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "v1").unwrap();
        copy_if_changed(&source, &copy).unwrap();
        assert_eq!(fs::read_to_string(&copy).unwrap(), "v1");
        let modified = fs::metadata(&copy).unwrap().modified().unwrap();
        copy_if_changed(&source, &copy).unwrap();
        assert_eq!(fs::metadata(&copy).unwrap().modified().unwrap(), modified);
        fs::write(&source, "v2").unwrap();
        copy_if_changed(&source, &copy).unwrap();
        assert_eq!(fs::read_to_string(&copy).unwrap(), "v2");
        fs::remove_dir_all(&folder).unwrap();
    }

    fn tempfile_folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("localhost-agents-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }
}
