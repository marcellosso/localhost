//! The app's own settings, kept in its config folder
//! (`~/.config/com.sixpetals.localhost.desktop/settings.json`). They are app
//! state, not part of the core, like the macOS app's UserDefaults.

use std::path::PathBuf;
use std::sync::Mutex;

use localhost_core::LocalhostError;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::AppState;
use crate::bridge::CommandResult;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// The command that opens a folder in the user's editor, e.g. `code`.
    /// Picked once; there is no default.
    pub editor_command: Option<String>,
    /// Whether a Service may move to a free port when its Port is taken.
    pub automatic_port_move: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { editor_command: None, automatic_port_move: true }
    }
}

pub struct SettingsStore {
    /// `None` when there is no config folder: settings then last for the session.
    path: Option<PathBuf>,
    settings: Mutex<Settings>,
}

impl SettingsStore {
    /// The settings in `path`; the defaults if it is missing or unreadable.
    pub fn open(path: Option<PathBuf>) -> SettingsStore {
        let settings = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        SettingsStore { path, settings: Mutex::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn save(&self, settings: Settings) -> Result<(), LocalhostError> {
        if let Some(path) = &self.path {
            let failed = |error: std::io::Error| LocalhostError::PersistenceFailed {
                path: path.display().to_string(),
                reason: error.to_string(),
            };
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder).map_err(failed)?;
            }
            let temporary = path.with_extension("json.tmp");
            let data = serde_json::to_vec_pretty(&settings).expect("settings serialize");
            std::fs::write(&temporary, data).map_err(failed)?;
            std::fs::rename(&temporary, path).map_err(failed)?;
        }
        *self.settings.lock().unwrap() = settings;
        Ok(())
    }
}

#[tauri::command]
#[specta::specta]
pub fn settings(state: tauri::State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Saves the settings and applies them.
#[tauri::command]
#[specta::specta]
pub fn save_settings(state: tauri::State<'_, AppState>, settings: Settings) -> CommandResult {
    let editor_command = settings.editor_command.as_deref().map(str::trim).filter(|command| !command.is_empty());
    let settings = Settings { editor_command: editor_command.map(String::from), ..settings };
    state.settings.save(settings.clone())?;
    if let Ok(core) = state.core() {
        core.services.set_automatic_port_move(settings.automatic_port_move);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_reopens_settings() {
        let folder = std::env::temp_dir().join(format!("localhost-settings-{}", std::process::id()));
        let path = folder.join("settings.json");
        let store = SettingsStore::open(Some(path.clone()));
        assert_eq!(store.get(), Settings::default());

        let changed = Settings { editor_command: Some("code".into()), automatic_port_move: false };
        store.save(changed.clone()).unwrap();
        assert_eq!(SettingsStore::open(Some(path)).get(), changed);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"editorCommand":"zed"}"#).unwrap();
        assert_eq!(settings, Settings { editor_command: Some("zed".into()), automatic_port_move: true });
    }
}
