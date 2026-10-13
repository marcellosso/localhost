//! The Tauri app for Linux (and later Windows): a thin shell that hosts the
//! Rust core in-process and bridges it to the React UI (ADR-0009). Commands
//! and events are typed: `src/bindings.ts` is generated from this crate.

mod bridge;
mod desktop;
mod launch;
mod listeners;
mod logs;
mod perf;
mod services;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use localhost_core::{
    LocalhostError, LogStore, Origin, ProjectLibrary, RunRegistry, ServiceManager, ServiceManagerOptions,
    SystemSecretStore, default_storage_root,
};
use tauri::{Manager, RunEvent};
use tauri_specta::collect_commands;

/// The core objects every window shares.
pub struct Core {
    library: Arc<ProjectLibrary>,
    services: ServiceManager,
    logs: Arc<LogStore>,
    runs: Arc<RunRegistry>,
}

impl Core {
    /// Opens the Projects in `root`. Needs a tokio runtime: the manager
    /// spawns a task watching the library.
    fn open(root: &Path) -> Result<Core, LocalhostError> {
        let library = Arc::new(ProjectLibrary::open(root, Arc::new(SystemSecretStore::new()))?);
        let logs = Arc::new(LogStore::new(root));
        let runs = Arc::new(RunRegistry::new(root));
        let services =
            ServiceManager::new(library.clone(), logs.clone(), runs.clone(), ServiceManagerOptions::default());
        Ok(Core { library, services, logs, runs })
    }
}

/// What the commands share: the core, or why it couldn't be opened (the UI
/// says so), the open log streams, the launch questions and the settings.
pub struct AppState {
    core: Result<Core, LocalhostError>,
    logs: logs::LogStreams,
    launch: Mutex<launch::Launch>,
    settings: settings::SettingsStore,
    perf: Option<perf::PerfSettings>,
}

impl AppState {
    fn core(&self) -> Result<&Core, LocalhostError> {
        self.core.as_ref().map_err(Clone::clone)
    }
}

/// Where this app keeps its data. On macOS that folder belongs to the
/// SwiftUI app, so a development build of this one keeps its own.
/// `LOCALHOST_STORAGE_ROOT` overrides both.
fn storage_root() -> PathBuf {
    if let Some(root) = std::env::var_os("LOCALHOST_STORAGE_ROOT") {
        return root.into();
    }
    let root = default_storage_root();
    if cfg!(target_os = "macos") { root.with_file_name("localhost-desktop") } else { root }
}

/// The commands and events the UI may use; also what `bindings.ts` is
/// generated from.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new().commands(collect_commands![
        bridge::subscribe,
        services::start_service,
        services::stop_service,
        services::restart_service,
        services::stop_holder_and_start,
        services::start_all,
        services::stop_all,
        listeners::listeners,
        listeners::stop_listener,
        launch::launch_checks,
        launch::dismiss_unreadable_files,
        launch::stop_orphans,
        launch::leave_orphans,
        desktop::open_service_url,
        desktop::open_folder,
        settings::settings,
        settings::save_settings,
        logs::open_log,
        logs::close_log,
        perf::perf_settings,
        perf::finish_perf_run,
    ])
}

pub fn run() {
    let builder = specta_builder();
    let perf = perf::PerfSettings::from_environment();
    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let core = tauri::async_runtime::block_on(async {
                match &perf {
                    Some(settings) => perf::open_core(settings),
                    None => Core::open(&storage_root()),
                }
            });
            // Before anything starts, so this session's Runs can't be mistaken for Orphans.
            let orphans = match &core {
                Ok(core) if perf.is_none() => core.runs.orphans(),
                _ => Vec::new(),
            };
            let settings = settings::SettingsStore::open(
                app.path().app_config_dir().ok().map(|folder| folder.join("settings.json")),
            );
            if let Ok(core) = &core {
                core.services.set_automatic_port_move(settings.get().automatic_port_move);
            }
            app.manage(AppState {
                core,
                logs: logs::LogStreams::default(),
                launch: launch::Launch::new(orphans),
                settings,
                perf,
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building localhost")
        .run(|app, event| {
            // Quitting stops every Service, as in the macOS app. Asking first
            // comes with the tray.
            if let RunEvent::Exit = event {
                let state = app.state::<AppState>();
                if let Ok(core) = state.core() {
                    tauri::async_runtime::block_on(core.services.stop_every_run(Origin::MainWindow));
                }
                if let Some(settings) = &state.perf {
                    perf::clean_up(settings);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use specta_typescript::{BigIntExportBehavior, Typescript};

    /// Writes `src/bindings.ts`. CI fails if that changes the committed file.
    #[test]
    fn exports_the_typescript_bindings() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");
        // Only ProcessStartTime is 64-bit; its values fit in a JS number.
        // Its unused helpers would fail `noUnusedLocals`.
        let typescript =
            Typescript::default().bigint(BigIntExportBehavior::Number).header("// @ts-nocheck: generated.");
        super::specta_builder().export(typescript, path).expect("bindings.ts is written");
        // tauri-specta rc.21 also exports Tauri's placeholder for `Channel`,
        // which clashes with the `Channel as TAURI_CHANNEL` import it adds.
        let bindings = std::fs::read_to_string(path).unwrap();
        std::fs::write(path, bindings.replace("export type TAURI_CHANNEL<TSend> = null\n", "")).unwrap();
    }
}
