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
    /// Whether localhost checks for a new version at launch and every few hours.
    pub automatic_update_checks: bool,
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
        Settings {
            editor_command: None,
            automatic_port_move: true,
            appearance: Appearance::System,
            log_font_size: 12,
            automatic_update_checks: true,
        }
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
    /// Whether Connect your agents opened by itself at a launch, as versions
    /// before the welcome did: also a sign the app was opened before.
    #[serde(default)]
    has_offered_agents: bool,
    /// The first-run welcome and checklist; `None` until the first launch
    /// that has them decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    onboarding: Option<Onboarding>,
}

/// Where the user is in the first run: whether the welcome was finished
/// or skipped, whether the Get started checklist is over (dismissed,
/// complete, or never shown to someone who used localhost before), and
/// whether a Service has been Running, which the checklist keeps ticked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Onboarding {
    pub has_seen_welcome: bool,
    pub is_checklist_done: bool,
    pub has_started_service: bool,
}

/// What the first run starts from: the saved record, if any, and whether
/// this app was opened before the welcome existed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingLaunch {
    pub onboarding: Option<Onboarding>,
    pub has_opened_before: bool,
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

    pub fn onboarding_launch(&self) -> OnboardingLaunch {
        let stored = self.stored.lock().unwrap();
        OnboardingLaunch {
            onboarding: stored.onboarding,
            has_opened_before: stored.has_offered_agents || stored.has_explained_close,
        }
    }

    /// Remembers the first run's progress; for this session only if the
    /// file can't be written, so the welcome never shows twice in one.
    pub fn set_onboarding(&self, onboarding: Onboarding) {
        let stored = Stored { onboarding: Some(onboarding), ..self.stored.lock().unwrap().clone() };
        if self.write(stored).is_err() {
            self.stored.lock().unwrap().onboarding = Some(onboarding);
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

/// Where the first run stands, and whether localhost was used before it existed.
#[tauri::command]
#[specta::specta]
pub fn onboarding(state: tauri::State<'_, AppState>) -> OnboardingLaunch {
    state.settings.onboarding_launch()
}

#[tauri::command]
#[specta::specta]
pub fn save_onboarding(state: tauri::State<'_, AppState>, onboarding: Onboarding) {
    state.settings.set_onboarding(onboarding);
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
            automatic_update_checks: false,
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
    fn remembers_the_first_run_and_who_opened_localhost_before_it() {
        let folder = std::env::temp_dir().join(format!("localhost-settings-onboarding-{}", std::process::id()));
        let path = folder.join("settings.json");
        let store = SettingsStore::open(Some(path.clone()));
        assert_eq!(store.onboarding_launch(), OnboardingLaunch { onboarding: None, has_opened_before: false });

        let onboarding = Onboarding { has_seen_welcome: true, ..Onboarding::default() };
        store.set_onboarding(onboarding);
        store.save(Settings { log_font_size: 14, ..Settings::default() }).unwrap();
        let reopened = SettingsStore::open(Some(path));
        assert_eq!(reopened.onboarding_launch().onboarding, Some(onboarding));
        assert_eq!(reopened.get().log_font_size, 14);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn an_earlier_version_s_file_counts_as_opened_before() {
        let folder = std::env::temp_dir().join(format!("localhost-settings-earlier-{}", std::process::id()));
        let path = folder.join("settings.json");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&path, r#"{"logFontSize":12,"hasOfferedAgents":true}"#).unwrap();
        let launch = SettingsStore::open(Some(path)).onboarding_launch();
        assert_eq!(launch, OnboardingLaunch { onboarding: None, has_opened_before: true });
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"editorCommand":"zed"}"#).unwrap();
        assert_eq!(settings, Settings { editor_command: Some("zed".into()), ..Settings::default() });
    }
}
