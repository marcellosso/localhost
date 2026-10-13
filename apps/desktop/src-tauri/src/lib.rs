//! The Tauri app for Linux (and later Windows): a thin shell that hosts the
//! Rust core in-process and bridges it to the React UI (ADR-0009). Commands
//! and events are typed: `src/bindings.ts` is generated from this crate.

mod bridge;

use std::path::PathBuf;
use std::sync::Arc;

use localhost_core::{
    LocalhostError, LogStore, ProjectLibrary, RunRegistry, ServiceManager, ServiceManagerOptions, SystemSecretStore,
    default_storage_root,
};
use tauri::Manager;
use tauri_specta::collect_commands;

/// The core objects every window shares.
pub struct Core {
    library: Arc<ProjectLibrary>,
    services: ServiceManager,
}

impl Core {
    /// Opens the Projects in `root`. Needs a tokio runtime: the manager
    /// spawns a task watching the library.
    fn open(root: &std::path::Path) -> Result<Core, LocalhostError> {
        let library = Arc::new(ProjectLibrary::open(root, Arc::new(SystemSecretStore::new()))?);
        let services = ServiceManager::new(
            library.clone(),
            Arc::new(LogStore::new(root)),
            Arc::new(RunRegistry::new(root)),
            ServiceManagerOptions::default(),
        );
        Ok(Core { library, services })
    }
}

/// The core, or why it couldn't be opened (the UI says so).
pub struct AppState(Result<Core, LocalhostError>);

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
    tauri_specta::Builder::new().commands(collect_commands![bridge::subscribe])
}

pub fn run() {
    let builder = specta_builder();
    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let core = tauri::async_runtime::block_on(async { Core::open(&storage_root()) });
            app.manage(AppState(core));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running localhost");
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
