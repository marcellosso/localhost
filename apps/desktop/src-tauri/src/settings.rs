//! The app's own settings, kept in its config folder
//! (`~/.config/io.github.marcellosso.localhost.desktop/settings.json`, on Windows
//! `%APPDATA%\io.github.marcellosso.localhost.desktop\settings.json`). They are app
//! state, not part of the core.

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
    pub appearance: Appearance,
    /// Points, for the log and the commands in Service rows.
    pub log_font_size: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    /// Follows the desktop's light or dark style.
    #[default]
    System,
    Light,
    Dark,
}

/// The log font sizes Settings offers, as on macOS.
pub const LOG_FONT_SIZES: [u8; 6] = [10, 11, 12, 13, 14, 16];

impl Default for Settings {
    fn default() -> Settings {
        Settings { editor_command: None, automatic_port_move: true, appearance: Appearance::System, log_font_size: 12 }
    }
}

/// What the file holds: the settings, and what the app has told the user
/// once, which isn't a setting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(flatten)]
    settings: Settings,
    /// Whether closing the window has said that localhost keeps running (`lifecycle`).
    #[serde(default)]
    has_explained_close: bool,
}

pub struct SettingsStore {
    /// `None` when there is no config folder: settings then last for the session.
    path: Option<PathBuf>,
    stored: Mutex<Stored>,
}

impl SettingsStore {
    /// The settings in `path`; the defaults if it is missing or unreadable.
    pub fn open(path: Option<PathBuf>) -> SettingsStore {
        let stored = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        SettingsStore { path, stored: Mutex::new(stored) }
    }

    pub fn get(&self) -> Settings {
        self.stored.lock().unwrap().settings.clone()
    }

    fn save(&self, settings: Settings) -> Result<(), LocalhostError> {
        let stored = Stored { settings, ..self.stored.lock().unwrap().clone() };
        self.write(stored)
    }

    pub fn has_explained_close(&self) -> bool {
        self.stored.lock().unwrap().has_explained_close
    }

    /// Remembers that closing the window has said so; for this session
    /// only if the file can't be written.
    pub fn set_explained_close(&self) {
        let stored = Stored { has_explained_close: true, ..self.stored.lock().unwrap().clone() };
        if self.write(stored).is_err() {
            self.stored.lock().unwrap().has_explained_close = true;
        }
    }

    fn write(&self, stored: Stored) -> Result<(), LocalhostError> {
        if let Some(path) = &self.path {
            let failed = |error: std::io::Error| LocalhostError::PersistenceFailed {
                path: path.display().to_string(),
                reason: error.to_string(),
            };
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder).map_err(failed)?;
            }
            let temporary = path.with_extension("json.tmp");
            let data = serde_json::to_vec_pretty(&stored).expect("settings serialize");
            std::fs::write(&temporary, data).map_err(failed)?;
            std::fs::rename(&temporary, path).map_err(failed)?;
        }
        *self.stored.lock().unwrap() = stored;
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
    let log_font_size = if LOG_FONT_SIZES.contains(&settings.log_font_size) {
        settings.log_font_size
    } else {
        Settings::default().log_font_size
    };
    let settings = Settings { editor_command: editor_command.map(String::from), log_font_size, ..settings };
    state.settings.save(settings.clone())?;
    if let Ok(core) = state.core() {
        core.targets.set_automatic_port_move(settings.automatic_port_move);
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

        let changed = Settings {
            editor_command: Some("code".into()),
            automatic_port_move: false,
            appearance: Appearance::Dark,
            log_font_size: 14,
        };
        store.save(changed.clone()).unwrap();
        assert_eq!(SettingsStore::open(Some(path)).get(), changed);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn remembers_explaining_the_close_apart_from_the_settings() {
        let folder = std::env::temp_dir().join(format!("localhost-settings-close-{}", std::process::id()));
        let path = folder.join("settings.json");
        let store = SettingsStore::open(Some(path.clone()));
        assert!(!store.has_explained_close());

        store.set_explained_close();
        let changed = Settings { editor_command: Some("code".into()), ..Settings::default() };
        store.save(changed.clone()).unwrap();
        let reopened = SettingsStore::open(Some(path));
        assert!(reopened.has_explained_close());
        assert_eq!(reopened.get(), changed);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"editorCommand":"zed"}"#).unwrap();
        assert_eq!(settings, Settings { editor_command: Some("zed".into()), ..Settings::default() });
    }
}
