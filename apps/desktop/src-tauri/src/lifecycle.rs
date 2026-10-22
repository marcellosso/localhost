//! The app's lifetime: closing the window keeps the app and its Services
//! running; quitting asks first if anything is running, then stops
//! everything.
//!
//! A closed window is destroyed, not hidden, so a day in the tray doesn't
//! keep a webview (and its log streams) alive. The tray's Open localhost,
//! and launching the app again (single instance), bring it back. Without a
//! tray (GNOME without the AppIndicator extension) launching again is the way.
//! The first close says so once (`close_requested`): on Windows the tray
//! icon starts hidden in the notification area's overflow, so nothing on
//! screen shows that localhost is still running.
//!
//! On Windows the window is a WebView2, which acts as a browser unless told
//! otherwise: `prepare` turns its browser keys off.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use localhost_core::{Id, Origin};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, CloseRequestApi, Manager, WebviewWindow, WebviewWindowBuilder, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_specta::Event;

use crate::AppState;

pub const MAIN_WINDOW: &str = "main";

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
    /// The first close's note is on screen; the window closes once it is dismissed.
    is_explaining_close: AtomicBool,
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
        prepare(&window);
        let _ = window.set_focus();
    }
}

/// The main window's size when it opens, as the macOS app's.
const PREFERRED_SIZE: (f64, f64) = (1280.0, 800.0);
/// Its smallest size, as `minWidth`/`minHeight` in `tauri.conf.json`.
const MINIMUM_SIZE: (f64, f64) = (720.0, 420.0);
/// How much of the screen's work area an opening window may cover at most.
const MAXIMUM_SHARE: f64 = 0.92;

/// The preferred size, made smaller where the work area (the screen without
/// the taskbar or panels) can't hold it, but never below the minimum.
/// Logical pixels.
fn fitted_size(work_area: (f64, f64)) -> (f64, f64) {
    let fit = |preferred: f64, available: f64, minimum: f64| preferred.min(available * MAXIMUM_SHARE).max(minimum);
    (fit(PREFERRED_SIZE.0, work_area.0, MINIMUM_SIZE.0), fit(PREFERRED_SIZE.1, work_area.1, MINIMUM_SIZE.1))
}

/// Sizes the window to fit its screen's work area and centres it there.
/// Windows and Linux don't keep a window on screen themselves, as macOS does.
fn fit_to_screen(window: &WebviewWindow) {
    let monitor = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let available = area.size.to_logical::<f64>(scale);
    let (width, height) = fitted_size((available.width, available.height));
    let size = tauri::LogicalSize::new(width, height).to_physical::<i32>(scale);
    let _ = window.set_size(size);
    // Wayland places windows itself and ignores this.
    let _ = window.set_position(tauri::PhysicalPosition::new(
        area.position.x + (area.size.width as i32 - size.width).max(0) / 2,
        area.position.y + (area.size.height as i32 - size.height).max(0) / 2,
    ));
}

/// Readies a new main window, sized to its screen, and shows it (it is
/// created hidden, so it doesn't appear at one size and jump to another).
/// On Windows, in release builds, WebView2's
/// browser keys are turned off: F5 and Ctrl+R reload, Ctrl+P prints, Ctrl+F
/// finds in the page, Alt+← goes back, F12 opens the developer tools.
/// Debug builds keep them, for the developer tools.
pub fn prepare(window: &WebviewWindow) {
    fit_to_screen(window);
    let _ = window.show();
    #[cfg(all(windows, not(debug_assertions)))]
    let _ = window.with_webview(|webview| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows::core::Interface;
        // SAFETY: COM calls on the webview's own thread, where `with_webview` runs this.
        let _ = unsafe {
            webview.controller().CoreWebView2().and_then(|core| core.Settings()).and_then(|settings| {
                settings.cast::<ICoreWebView2Settings3>()?.SetAreBrowserAcceleratorKeysEnabled(false)
            })
        };
    });
    let _ = window;
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

/// What the first close says: where localhost is now, and how to quit it.
#[cfg(windows)]
const CLOSE_NOTE: &str = "Its services keep running. Open localhost or quit it from its icon in the notification area \
     on the taskbar, which may be hidden under ^.";
#[cfg(not(windows))]
const CLOSE_NOTE: &str =
    "Its services keep running. Open localhost or quit it from its tray icon, or launch localhost again to open it.";

/// The first time the main window is closed, holds the close and says
/// that localhost keeps running, once. The window closes when the note is
/// dismissed. Not on macOS, whose menu bar shows the icon.
pub fn close_requested(window: &Window, api: &CloseRequestApi) {
    if cfg!(target_os = "macos") || window.label() != MAIN_WINDOW {
        return;
    }
    let state = window.state::<AppState>();
    let lifecycle = &state.lifecycle;
    if lifecycle.is_exiting.load(Ordering::SeqCst) || state.settings.has_explained_close() {
        return;
    }
    api.prevent_close();
    if lifecycle.is_explaining_close.swap(true, Ordering::SeqCst) {
        return;
    }
    let window = window.clone();
    window
        .dialog()
        .message(CLOSE_NOTE)
        .title("localhost is still running")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok)
        .parent(&window)
        .show(move |_| {
            let state = window.state::<AppState>();
            state.settings.set_explained_close();
            state.lifecycle.is_explaining_close.store(false, Ordering::SeqCst);
            let _ = window.destroy();
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

#[cfg(test)]
mod tests {
    use super::fitted_size;

    #[test]
    fn opens_at_the_macos_apps_size_or_smaller_on_a_small_screen() {
        // 1920×1080 less a taskbar: room for the preferred size.
        assert_eq!(fitted_size((1920.0, 1040.0)), (1280.0, 800.0));
        // A 1366×768 laptop: 92% of the work area.
        let (width, height) = fitted_size((1366.0, 728.0));
        assert_eq!((width.round(), height.round()), (1257.0, 670.0));
        // Never below the minimum size.
        assert_eq!(fitted_size((640.0, 400.0)), (720.0, 420.0));
    }
}
