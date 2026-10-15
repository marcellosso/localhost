//! The app's lifetime, as in the macOS app (`docs/v1-decisions.md`, App
//! lifetime): closing the window keeps the app and its Services running;
//! quitting asks first if anything is running, then stops everything.
//!
//! A closed window is destroyed, not hidden, so a day in the tray doesn't
//! keep a webview (and its log streams) alive. The tray's Open localhost,
//! and launching the app again (single instance), bring it back. Without a
//! tray (GNOME without the AppIndicator extension) launching again is the way.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use localhost_core::{Id, Origin};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_specta::Event;

use crate::AppState;

const MAIN_WINDOW: &str = "main";

/// Where the window should go when the tray opens it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type, Event)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reveal {
    /// A Project, and one of its Services if given.
    #[serde(rename_all = "camelCase")]
    Project {
        project_id: Id,
        service_id: Option<Id>,
    },
    Ports,
}

#[derive(Default)]
pub struct Lifecycle {
    /// The quit was confirmed (or needed no asking): let the exit through.
    is_exiting: AtomicBool,
    /// The quit question is on screen, so asking again does nothing.
    is_asking: AtomicBool,
    /// Where a window being created should go once its UI has started.
    pending_reveal: Mutex<Option<Reveal>>,
}

/// Brings the window forward, creating it if it was closed, and shows `reveal`.
pub fn show_window(app: &AppHandle, reveal: Option<Reveal>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(reveal) = reveal {
            let _ = reveal.emit_to(app, MAIN_WINDOW);
        }
        return;
    }
    let lifecycle = &app.state::<AppState>().lifecycle;
    *lifecycle.pending_reveal.lock().unwrap() = reveal;
    let config = app.config().app.windows.iter().find(|window| window.label == MAIN_WINDOW);
    if let Some(config) = config
        && let Ok(builder) = WebviewWindowBuilder::from_config(app, config)
        && let Ok(window) = builder.build()
    {
        let _ = window.set_focus();
    }
}

/// Quits, asking first if any Service is active. The stops that follow
/// carry `origin`.
pub fn quit(app: &AppHandle, origin: Origin) {
    let state = app.state::<AppState>();
    let lifecycle = &state.lifecycle;
    if lifecycle.is_exiting.load(Ordering::SeqCst) || lifecycle.is_asking.swap(true, Ordering::SeqCst) {
        return;
    }
    let count = state.core().map_or(0, |core| {
        let projects = core.library.projects();
        let services = projects.iter().flat_map(|project| &project.services);
        services.filter(|service| core.targets.status(service.id).state.is_active()).count()
    });
    if count == 0 {
        exit(app, origin);
        return;
    }
    let title =
        if count == 1 { "Stop 1 service and quit?".to_string() } else { format!("Stop {count} services and quit?") };
    let mut dialog = app
        .dialog()
        .message("Running services are stopped before localhost quits.")
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Stop and Quit".into(), "Cancel".into()));
    // Over the window when it is open; on its own when quitting from the tray.
    if let Some(window) = visible_window(app) {
        dialog = dialog.parent(&window);
    }
    let app = app.clone();
    dialog.show(move |is_confirmed| {
        app.state::<AppState>().lifecycle.is_asking.store(false, Ordering::SeqCst);
        if is_confirmed {
            exit(&app, origin);
        }
    });
}

/// Stops every Run, then exits without asking.
fn exit(app: &AppHandle, origin: Origin) {
    app.state::<AppState>().lifecycle.is_exiting.store(true, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(core) = app.state::<AppState>().core() {
            core.targets.stop_every_run(origin).await;
        }
        app.exit(0);
    });
}

/// Exits now, e.g. once a performance run has reported.
pub fn exit_now(app: &AppHandle) {
    app.state::<AppState>().lifecycle.is_exiting.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Whether an exit Tauri is about to make may go ahead. `code` is `None`
/// when the last window closed, which keeps the app running; an
/// `AppHandle::exit` that didn't come through `quit` asks first.
pub fn should_exit(app: &AppHandle, code: Option<i32>) -> bool {
    if app.state::<AppState>().lifecycle.is_exiting.load(Ordering::SeqCst) {
        return true;
    }
    if code.is_some() {
        quit(app, Origin::MainWindow);
    }
    false
}

fn visible_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW).filter(|window| window.is_visible().unwrap_or(false))
}

/// Ctrl+Q in the window.
#[tauri::command]
#[specta::specta]
pub fn quit_app(app: AppHandle) {
    quit(&app, Origin::MainWindow);
}

/// Where the tray asked a new window to go, once.
#[tauri::command]
#[specta::specta]
pub fn take_reveal(state: tauri::State<'_, AppState>) -> Option<Reveal> {
    state.lifecycle.pending_reveal.lock().unwrap().take()
}
