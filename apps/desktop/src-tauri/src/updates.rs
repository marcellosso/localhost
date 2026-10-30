//! Updates from GitHub Releases. The release workflow publishes
//! `latest.json` with every release, each package signed with the
//! updater key; the app checks it at launch and every few hours (unless
//! turned off in Settings), and on Check for Updates. A newer version shows
//! a notice; Update Now downloads it in the background and verifies it
//! against the public key built into the app; Restart to Update installs it.
//!
//! The AppImage and the Windows installer update in place. A `.deb` or an
//! `.rpm` belongs to the package manager, so its notice links to the
//! release instead. A build without the public key (any local build)
//! doesn't check at all.
//!
//! Installing restarts the app, so, like quitting, it stops every Service
//! first, after saying which ones are running, and disconnects the coding
//! agents connected through `localhost mcp`, after saying how many: on
//! Windows the running `localhost.exe` couldn't be replaced, and anywhere an
//! agent kept on would reach the old core. Then it retires the shared
//! core, so the new version starts a core of its own instead of reaching
//! the old one; on Windows the installer would close that core anyway, as
//! it runs from the app's executable.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use localhost_core::{Id, Origin, Project, ServiceStatus, UpdateConfirmation};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::{AppState, lifecycle};

/// How long after launch the first automatic check waits, so it doesn't
/// compete with the window opening.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
/// How often automatic checks run.
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// How often the automatic checker wakes to see whether a check is due.
/// Wall-clock time, so a check missed while the computer slept runs soon
/// after it wakes.
const CHECKER_TICK: Duration = Duration::from_secs(15 * 60);
/// How often download progress reaches the window, at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Where a release's notes are when `latest.json` doesn't say.
const RELEASES: &str = "https://github.com/marcellosso/localhost/releases";

/// What the update notice shows; also the event sent when it changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type, Event)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateState {
    /// No newer version known, or the notice was put off with Later.
    #[default]
    None,
    /// A newer version. `can_install` is false where a package manager
    /// installs it (`.deb`, `.rpm`); `problem` says why a download failed.
    #[serde(rename_all = "camelCase")]
    Available { version: String, notes_url: String, can_install: bool, problem: Option<String> },
    /// `progress` is from 0 to 1, `None` while the size isn't known.
    #[serde(rename_all = "camelCase")]
    Downloading { version: String, notes_url: String, progress: Option<f64> },
    /// Downloaded and verified: restarting installs it.
    #[serde(rename_all = "camelCase")]
    Ready { version: String, notes_url: String },
}

/// What Check for Updates found.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateCheck {
    /// The notice shows it.
    Available {
        version: String,
    },
    UpToDate {
        version: String,
    },
    /// The check couldn't be made, or this build doesn't update itself.
    Failed {
        message: String,
    },
}

#[derive(Default)]
pub struct Updates {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    state: UpdateState,
    /// The update the notice offers, and its package once downloaded.
    update: Option<Update>,
    package: Option<Vec<u8>>,
    /// The version Later put off; automatic checks don't offer it again.
    put_off: Option<String>,
    last_check: Option<SystemTime>,
    is_checking: bool,
    is_installing: bool,
}

impl Updates {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn state(&self) -> UpdateState {
        self.inner().state.clone()
    }
}

/// Whether this build can check for updates: only the release workflow
/// builds it with the updater's public key.
fn is_configured(app: &AppHandle) -> bool {
    let config = app.config().plugins.0.get("updater");
    config
        .and_then(|config| config.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty())
}

/// Whether this package replaces itself: the AppImage and the Windows
/// installer do, the package manager installs a `.deb` or an `.rpm`.
fn installs_itself(bundle: Option<BundleType>) -> bool {
    matches!(bundle, Some(BundleType::AppImage | BundleType::Nsis))
}

/// The release's page: `releaseNotesUrl` in `latest.json`, which the
/// release workflow writes, else the tag's page on GitHub.
fn notes_url(manifest: &serde_json::Value, version: &str) -> String {
    match manifest.get("releaseNotesUrl").and_then(|url| url.as_str()) {
        Some(url) if url.starts_with("https://") => url.to_string(),
        _ => format!("{RELEASES}/tag/v{version}"),
    }
}

fn set_state(app: &AppHandle, inner: &mut Inner, state: UpdateState) {
    if inner.state != state {
        inner.state = state.clone();
        let _ = state.emit(app);
    }
}

/// Checks at launch and then every few hours, while Settings allows it.
pub fn watch(app: &AppHandle) {
    if !is_configured(app) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if is_check_due(&app) {
                // An automatic check's failure (offline, say) waits for the next one.
                if let Err(message) = check(&app, false).await {
                    eprintln!("localhost: couldn't check for updates: {message}");
                }
            }
            tokio::time::sleep(CHECKER_TICK).await;
        }
    });
}

fn is_check_due(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if !state.settings.get().automatic_update_checks {
        return false;
    }
    let inner = state.updates.inner();
    let is_busy = matches!(inner.state, UpdateState::Downloading { .. } | UpdateState::Ready { .. });
    let is_recent = inner.last_check.and_then(|last| last.elapsed().ok()).is_some_and(|since| since < CHECK_INTERVAL);
    !is_busy && !is_recent
}

/// Asks for `latest.json` and updates the notice. A version put off with
/// Later shows again only when `is_manual`.
async fn check(app: &AppHandle, is_manual: bool) -> Result<Option<String>, String> {
    let updates = &app.state::<AppState>().updates;
    {
        let mut inner = updates.inner();
        if inner.is_checking {
            return Err("localhost is already checking for updates.".into());
        }
        inner.is_checking = true;
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await.map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    let mut inner = updates.inner();
    inner.is_checking = false;
    inner.last_check = Some(SystemTime::now());
    let update = found?;
    let Some(update) = update else {
        if matches!(inner.state, UpdateState::Available { .. }) {
            inner.update = None;
            set_state(app, &mut inner, UpdateState::None);
        }
        return Ok(None);
    };
    let version = update.version.clone();
    // Already downloading or downloaded: keep going.
    let is_known = inner.update.as_ref().is_some_and(|known| known.version == version);
    if is_known && !matches!(inner.state, UpdateState::Available { .. } | UpdateState::None) {
        return Ok(Some(version));
    }
    if is_manual {
        inner.put_off = None;
    }
    let state = if inner.put_off.as_deref() == Some(version.as_str()) {
        UpdateState::None
    } else {
        UpdateState::Available {
            version: version.clone(),
            notes_url: notes_url(&update.raw_json, &version),
            can_install: installs_itself(tauri::utils::platform::bundle_type()),
            problem: None,
        }
    };
    inner.update = Some(update);
    inner.package = None;
    set_state(app, &mut inner, state);
    Ok(Some(version))
}

/// The notice's current state, e.g. for a window that just opened.
#[tauri::command]
#[specta::specta]
pub fn update_state(state: tauri::State<'_, AppState>) -> UpdateState {
    state.updates.state()
}

/// Check for Updates: checks now, and shows the notice again even for a
/// version put off with Later.
#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(app: AppHandle) -> UpdateCheck {
    if !is_configured(&app) {
        return UpdateCheck::Failed {
            message: "This build of localhost doesn’t update itself. Download new versions from GitHub Releases."
                .into(),
        };
    }
    match check(&app, true).await {
        Ok(Some(version)) => UpdateCheck::Available { version },
        Ok(None) => UpdateCheck::UpToDate { version: app.package_info().version.to_string() },
        Err(message) => UpdateCheck::Failed {
            message: format!("localhost couldn’t check for updates: {message}. Check your connection and try again."),
        },
    }
}

/// Later: hides the notice until a newer version, or Check for Updates.
#[tauri::command]
#[specta::specta]
pub fn put_off_update(app: AppHandle) {
    let updates = &app.state::<AppState>().updates;
    let mut inner = updates.inner();
    if let UpdateState::Available { version, .. } = &inner.state {
        inner.put_off = Some(version.clone());
        set_state(&app, &mut inner, UpdateState::None);
    }
}

/// Update Now: downloads the update in the background and verifies its
/// signature. Nothing is installed until Restart to Update.
#[tauri::command]
#[specta::specta]
pub fn download_update(app: AppHandle) {
    let updates = &app.state::<AppState>().updates;
    let (update, notes_url) = {
        let mut inner = updates.inner();
        let UpdateState::Available { notes_url, can_install: true, .. } = inner.state.clone() else { return };
        let Some(update) = inner.update.clone() else { return };
        let state =
            UpdateState::Downloading { version: update.version.clone(), notes_url: notes_url.clone(), progress: None };
        set_state(&app, &mut inner, state);
        (update, notes_url)
    };
    tauri::async_runtime::spawn(async move {
        let version = update.version.clone();
        let mut received = 0_u64;
        let mut last_progress = Instant::now();
        let progress_app = app.clone();
        let progress_notes = notes_url.clone();
        let downloaded = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    if last_progress.elapsed() < PROGRESS_INTERVAL {
                        return;
                    }
                    last_progress = Instant::now();
                    let progress =
                        total.filter(|total| *total > 0).map(|total| (received as f64 / total as f64).min(1.0));
                    let updates = &progress_app.state::<AppState>().updates;
                    let mut inner = updates.inner();
                    if matches!(inner.state, UpdateState::Downloading { .. }) {
                        let state = UpdateState::Downloading {
                            version: version.clone(),
                            notes_url: progress_notes.clone(),
                            progress,
                        };
                        set_state(&progress_app, &mut inner, state);
                    }
                },
                || {},
            )
            .await;
        let updates = &app.state::<AppState>().updates;
        let mut inner = updates.inner();
        let state = match downloaded {
            Ok(package) => {
                inner.package = Some(package);
                UpdateState::Ready { version: update.version.clone(), notes_url }
            }
            Err(error) => UpdateState::Available {
                version: update.version.clone(),
                notes_url,
                can_install: true,
                problem: Some(format!("The download failed: {error}. Try again.")),
            },
        };
        set_state(&app, &mut inner, state);
    });
}

/// Restart to Update: asks first if any Service is active or any coding
/// agent is connected, since the restart stops the Services and
/// disconnects the agents, then installs and relaunches.
#[tauri::command]
#[specta::specta]
pub fn install_update(app: AppHandle) {
    let state = app.state::<AppState>();
    let version = match &state.updates.state() {
        UpdateState::Ready { version, .. } => version.clone(),
        _ => return,
    };
    let active = state.core().map_or_else(
        |_| Vec::new(),
        |core| {
            let statuses: HashMap<Id, ServiceStatus> = core
                .library
                .projects()
                .iter()
                .flat_map(|project| &project.services)
                .map(|service| (service.id, core.targets.status(service.id)))
                .collect();
            active_services(&core.library.projects(), &statuses)
        },
    );
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let agents = match app.state::<AppState>().core() {
            Ok(core) => core.targets.connected_agent_count().await,
            Err(_) => 0,
        };
        let services = active.iter().map(|(_, services)| services.len()).sum::<usize>();
        let Some(confirmation) = UpdateConfirmation::new(services, agents) else {
            install(&app);
            return;
        };
        let mut message = format!("localhost restarts to install version {version}.");
        if !active.is_empty() {
            message += &format!(
                " These services are stopped first:\n\n{}",
                active
                    .iter()
                    .map(|(project, services)| format!("{project}: {}", services.join(", ")))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        if let Some(note) = &confirmation.agents_note {
            message += &format!("\n\n{}", note.replace('`', ""));
        }
        let mut dialog = app
            .dialog()
            .message(message)
            .title(confirmation.title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(confirmation.button, "Cancel".into()));
        if let Some(window) = app.get_webview_window(lifecycle::MAIN_WINDOW) {
            dialog = dialog.parent(&window);
        }
        let app = app.clone();
        dialog.show(move |is_confirmed| {
            if is_confirmed {
                install(&app);
            }
        });
    });
}

/// The active Services' names by Project, in the library's order.
fn active_services(projects: &[Project], statuses: &HashMap<Id, ServiceStatus>) -> Vec<(String, Vec<String>)> {
    projects
        .iter()
        .filter_map(|project| {
            let names: Vec<String> = project
                .services
                .iter()
                .filter(|service| statuses.get(&service.id).is_some_and(|status| status.state.is_active()))
                .map(|service| service.name.clone())
                .collect();
            (!names.is_empty()).then(|| (project.name.clone(), names))
        })
        .collect()
}

/// Stops every Run and retires the shared core, then installs: the
/// AppImage is replaced and the app relaunched; on Windows the installer
/// takes over and relaunches it.
fn install(app: &AppHandle) {
    let (update, package) = {
        let updates = &app.state::<AppState>().updates;
        let mut inner = updates.inner();
        if inner.is_installing {
            return;
        }
        let (Some(update), Some(package)) = (inner.update.clone(), inner.package.clone()) else { return };
        inner.is_installing = true;
        (update, package)
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Ok(core) = state.core() {
            core.targets.stop_every_run(Origin::MainWindow).await;
            core.targets.disconnect_agents().await;
            core.targets.retire_shared_core().await;
        }
        match update.install(&package) {
            Ok(()) => {
                lifecycle::allow_exit(&app);
                app.restart();
            }
            Err(error) => {
                let mut inner = state.updates.inner();
                inner.is_installing = false;
                let failed = UpdateState::Available {
                    version: update.version.clone(),
                    notes_url: notes_url(&update.raw_json, &update.version),
                    can_install: true,
                    problem: Some(format!("The update couldn’t be installed: {error}.")),
                };
                inner.package = None;
                set_state(&app, &mut inner, failed);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use localhost_core::{Key, MemorySecretStore, NewService, ProjectLibrary, ServiceState};
    use tauri_plugin_updater::RemoteRelease;

    use super::*;

    /// The shape of the `latest.json` that `scripts/updater-manifest.sh` writes.
    const MANIFEST: &str = r#"{
        "version": "0.5.0",
        "notes": "localhost 0.5.0",
        "pub_date": "2026-11-02T10:00:00Z",
        "releaseNotesUrl": "https://github.com/marcellosso/localhost/releases/tag/v0.5.0",
        "platforms": {
            "linux-x86_64-appimage": { "url": "https://example.com/a.AppImage", "signature": "a" },
            "linux-x86_64-deb": { "url": "https://example.com/a.deb", "signature": "d" },
            "linux-x86_64-rpm": { "url": "https://example.com/a.rpm", "signature": "r" },
            "windows-x86_64-nsis": { "url": "https://example.com/a-setup.exe", "signature": "w" }
        }
    }"#;

    #[test]
    fn the_manifest_names_each_package_by_its_bundle() {
        let manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        let release: RemoteRelease = serde_json::from_value(manifest.clone()).unwrap();
        assert_eq!(release.version.to_string(), "0.5.0");
        assert_eq!(release.download_url("linux-x86_64-appimage").unwrap().as_str(), "https://example.com/a.AppImage");
        assert_eq!(release.signature("windows-x86_64-nsis").unwrap(), "w");
        // No bundle-less entry: a `.deb` must never find the AppImage.
        assert!(release.download_url("linux-x86_64").is_err());
        assert_eq!(notes_url(&manifest, "0.5.0"), "https://github.com/marcellosso/localhost/releases/tag/v0.5.0");
    }

    #[test]
    fn notes_default_to_the_tag_page() {
        assert_eq!(notes_url(&serde_json::json!({}), "1.2.3"), format!("{RELEASES}/tag/v1.2.3"));
        let insecure = serde_json::json!({ "releaseNotesUrl": "http://example.com" });
        assert_eq!(notes_url(&insecure, "1.2.3"), format!("{RELEASES}/tag/v1.2.3"));
    }

    #[test]
    fn only_the_appimage_and_the_windows_installer_install_themselves() {
        assert!(installs_itself(Some(BundleType::AppImage)));
        assert!(installs_itself(Some(BundleType::Nsis)));
        assert!(!installs_itself(Some(BundleType::Deb)));
        assert!(!installs_itself(Some(BundleType::Rpm)));
        assert!(!installs_itself(None));
    }

    #[test]
    fn lists_the_active_services_by_project() {
        let root = std::env::temp_dir().join(format!("localhost-updates-{}-{}", std::process::id(), Id::new()));
        std::fs::create_dir_all(&root).unwrap();
        let library = ProjectLibrary::open(&root, Arc::new(MemorySecretStore::new())).unwrap();
        let add = |name: &str, services: &[&str]| {
            let key = |name: &str| Key::new(name.to_lowercase()).unwrap();
            let services = services.iter().map(|service| NewService::new(*service, key(service), "sleep 1")).collect();
            library.add_project(name, key(name), &root.to_string_lossy(), services, Origin::Cli).unwrap()
        };
        let shop = add("Shop", &["Web", "API", "Worker"]);
        let blog = add("Blog", &["Site"]);
        let status = |service_id: Id, state: ServiceState| {
            (service_id, ServiceStatus { state, ..ServiceStatus::never_run(service_id) })
        };
        let statuses = HashMap::from([
            status(shop.services[0].id, ServiceState::Running),
            status(shop.services[1].id, ServiceState::Failed),
            status(shop.services[2].id, ServiceState::Stopping),
            status(blog.services[0].id, ServiceState::Exited),
        ]);

        assert_eq!(
            active_services(&library.projects(), &statuses),
            vec![("Shop".to_string(), vec!["Web".to_string(), "Worker".to_string()])]
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
