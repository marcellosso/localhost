//! The Tauri app for Linux and Windows: a thin shell that hosts the
//! Rust core in-process and bridges it to the React UI. Commands
//! and events are typed: `src/bindings.ts` is generated from this crate.
//!
//! Services are reached through `Targets`, whatever their Project's
//! Execution Target: this machine through the shared core, a WSL distro
//! through its `localhostd` (`targets`).

mod agents;
mod bridge;
mod desktop;
mod dialogs;
mod editors;
mod launch;
mod lifecycle;
mod listeners;
mod logs;
mod palette;
mod perf;
mod services;
mod settings;
mod targets;
mod tray;
mod updates;
mod wsl;

pub use targets::DAEMON_ARGUMENT;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use localhost_core::{
    Launcher, LocalhostError, Origin, ProjectLibrary, SecretStore, ServiceManagerOptions, SystemSecretStore, Targets,
    default_storage_root,
};
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_specta::{collect_commands, collect_events};

/// The core objects every window shares: the Project Configuration, and
/// the Execution Targets its Services run on.
pub struct Core {
    library: Arc<ProjectLibrary>,
    targets: Targets,
}

impl Core {
    /// Opens the Projects in `root`. Needs a tokio runtime: the Service
    /// manager spawns a task watching the library.
    fn open(root: &Path) -> Result<Core, LocalhostError> {
        Core::open_with(root, Arc::new(SystemSecretStore::new()), targets::launcher(root))
    }

    /// With this machine's Services in this process: a test's executable
    /// can't be started as the shared core.
    #[cfg(test)]
    fn open_in_process(root: &Path, secrets: Arc<dyn SecretStore>) -> Result<Core, LocalhostError> {
        Core::open_with(root, secrets, Box::new(|_| localhost_core::Link::InProcess))
    }

    fn open_with(root: &Path, secrets: Arc<dyn SecretStore>, launcher: Launcher) -> Result<Core, LocalhostError> {
        let library = Arc::new(ProjectLibrary::open(root, secrets)?);
        let targets = Targets::open(library.clone(), root, ServiceManagerOptions::default(), launcher);
        Ok(Core { library, targets })
    }
}

/// What the commands share: the core, or why it couldn't be opened (the UI
/// says so), the open log streams, the launch questions, the settings, the
/// update on offer and the app's lifetime.
pub struct AppState {
    core: Result<Core, LocalhostError>,
    logs: logs::LogStreams,
    subscriptions: bridge::Subscriptions,
    lifecycle: lifecycle::Lifecycle,
    launch: Mutex<launch::Launch>,
    settings: settings::SettingsStore,
    updates: updates::Updates,
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
    tauri_specta::Builder::new()
        .commands(collect_commands![
            bridge::subscribe,
            services::start_service,
            services::stop_service,
            services::restart_service,
            services::stop_holder_and_start,
            services::start_all,
            services::stop_all,
            services::run_stats,
            listeners::listeners,
            listeners::stop_listener,
            launch::launch_checks,
            launch::dismiss_unreadable_files,
            launch::stop_orphans,
            launch::leave_orphans,
            desktop::open_service_url,
            desktop::open_url,
            desktop::open_folder,
            editors::suggested_project_key,
            editors::suggested_service_key,
            editors::key_from_name,
            editors::add_project,
            editors::edit_project,
            editors::set_favorite,
            editors::remove_project,
            editors::add_service,
            editors::edit_service,
            editors::add_services,
            editors::command_line_uses_port,
            editors::remove_service,
            editors::service_relations,
            editors::scan_folder,
            editors::scan_project,
            editors::save_environment,
            editors::set_active_profile,
            editors::secret_value,
            editors::read_project_file,
            editors::import_project,
            editors::export_project,
            dialogs::choose_folder,
            dialogs::choose_working_directory,
            dialogs::choose_project_file,
            dialogs::choose_export_file,
            wsl::wsl_distros,
            settings::settings,
            settings::save_settings,
            logs::open_log,
            logs::close_log,
            logs::clear_logs,
            palette::fuzzy_scores,
            perf::perf_settings,
            perf::finish_perf_run,
            lifecycle::quit_app,
            lifecycle::take_reveal,
            updates::update_state,
            updates::check_for_updates,
            updates::put_off_update,
            updates::download_update,
            updates::install_update,
            agents::take_connect_agents_offer,
            agents::agent_statuses,
            agents::connect_agent,
            agents::disconnect_agent,
            agents::command_line_status,
            agents::add_command_to_path,
            agents::remove_command_from_path,
        ])
        .events(collect_events![lifecycle::Reveal, launch::OrphansFound, updates::UpdateState])
}

pub fn run() {
    let builder = specta_builder();
    let perf = perf::PerfSettings::from_environment();
    tauri::Builder::default()
        // First, so a second launch only brings the window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| lifecycle::show_window(app, None)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let core = tauri::async_runtime::block_on(async {
                match &perf {
                    Some(settings) => perf::open_core(settings),
                    None => Core::open(&storage_root()),
                }
            });
            // Before anything starts, so this session's Runs can't be mistaken
            // for Orphans. Only when they run in-process: the shared core
            // reports its own as it connects (`launch::watch_found_orphans`).
            let orphans = match &core {
                Ok(core) if perf.is_none() => core.targets.orphans(),
                _ => Vec::new(),
            };
            let is_measuring = perf.is_some();
            let settings = settings::SettingsStore::open(
                app.path().app_config_dir().ok().map(|folder| folder.join("settings.json")),
            );
            if let Ok(core) = &core {
                core.targets.set_automatic_port_move(settings.get().automatic_port_move);
            }
            app.manage(AppState {
                core,
                logs: logs::LogStreams::default(),
                subscriptions: bridge::Subscriptions::default(),
                lifecycle: lifecycle::Lifecycle::default(),
                launch: launch::Launch::new(orphans),
                settings,
                updates: updates::Updates::default(),
                perf,
            });
            if let Some(window) = app.get_webview_window(lifecycle::MAIN_WINDOW) {
                lifecycle::prepare(&window);
            }
            tray::install(app)?;
            agents::prepare();
            launch::watch_found_orphans(app.handle());
            if !is_measuring {
                updates::watch(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => lifecycle::close_requested(window, api),
            // A closed window's streams have no one to send to.
            WindowEvent::Destroyed => {
                let state = window.state::<AppState>();
                state.subscriptions.close_all();
                state.logs.close_all();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building localhost")
        .run(|app, event| match event {
            // Closing the window keeps the app running; quitting asks first (`lifecycle`).
            RunEvent::ExitRequested { code, api, .. } if !lifecycle::should_exit(app, code) => api.prevent_exit(),
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { has_visible_windows: false, .. } => lifecycle::show_window(app, None),
            RunEvent::Exit => {
                // Quitting has stopped every Run already; this catches any other way out.
                let state = app.state::<AppState>();
                if let Ok(core) = state.core() {
                    tauri::async_runtime::block_on(core.targets.stop_every_run(Origin::MainWindow));
                }
                if let Some(settings) = &state.perf {
                    perf::clean_up(settings);
                }
            }
            _ => {}
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
