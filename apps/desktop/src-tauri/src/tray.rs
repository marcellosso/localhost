//! The tray icon and its menu: the macOS menu bar companion's content
//! (`docs/localhost-v1.md` §8) as a native menu, since an AppIndicator can
//! only show a menu. Projects with any active or Failed Service, plus
//! favourites; Start / Stop / Restart and Open in Browser per Service, Start
//! All / Stop All per Project; warnings about Unmanaged Listeners holding a
//! configured Port; a red dot on the icon while any Service is Failed.
//!
//! It reads the core directly, not the window, so it stays current while the
//! window is closed. Its actions carry `Origin::MenuBar`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use localhost_core::{Id, LocalhostError, Origin, PortWarning, Project, ServiceState, ServiceStatus, port_warnings};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{App, AppHandle, Manager, Wry};

use crate::lifecycle::{self, Reveal};
use crate::{AppState, Core, desktop};

/// How often Listeners are scanned for the Port warnings. A native menu
/// can't say when it opens, unlike the macOS panel, which scans while open.
const LISTENER_SCAN: Duration = Duration::from_secs(5);
/// Changes this close together rebuild the menu once.
const SETTLE: Duration = Duration::from_millis(100);
/// Longer failure messages are cut, so the menu stays narrow.
const MAX_SUMMARY: usize = 60;

/// What a menu item does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Show(Reveal),
    Start(Id),
    Stop(Id),
    Restart(Id),
    OpenUrl(Id),
    StartAll(Id),
    StopAll(Id),
    OpenWindow,
    Quit,
}

/// The menu's content, compared to the last one so an unchanged menu isn't
/// rebuilt (rebuilding closes it while open on some desktops).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrayMenu {
    warnings: Vec<String>,
    projects: Vec<ProjectSection>,
    has_failure: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectSection {
    id: Id,
    /// e.g. "My SaaS ★".
    label: String,
    actions: Vec<(&'static str, Action)>,
    services: Vec<ServiceRow>,
}

#[derive(Clone, Debug, PartialEq)]
struct ServiceRow {
    /// e.g. "● Web — Running · :3000".
    label: String,
    actions: Vec<(&'static str, Action)>,
}

impl TrayMenu {
    pub fn new(projects: &[Project], statuses: &HashMap<Id, ServiceStatus>, warnings: &[PortWarning]) -> TrayMenu {
        let state = |id: Id| statuses.get(&id).map_or(ServiceState::Stopped, |status| status.state);
        let shown = projects.iter().filter(|project| {
            project.is_favorite
                || project.services.iter().any(|service| {
                    let state = state(service.id);
                    state.is_active() || state == ServiceState::Failed
                })
        });
        TrayMenu {
            warnings: warnings.iter().map(|warning| warning_text(warning, projects)).collect(),
            projects: shown.map(|project| ProjectSection::new(project, statuses)).collect(),
            has_failure: projects
                .iter()
                .flat_map(|project| &project.services)
                .any(|service| state(service.id) == ServiceState::Failed),
        }
    }
}

impl ProjectSection {
    fn new(project: &Project, statuses: &HashMap<Id, ServiceStatus>) -> ProjectSection {
        let is_any_active = project
            .services
            .iter()
            .any(|service| statuses.get(&service.id).is_some_and(|status| status.state.is_active()));
        let mut actions =
            vec![("Show in localhost", Action::Show(Reveal::Project { project_id: project.id, service_id: None }))];
        if !project.services.is_empty() {
            actions.push(("Start All", Action::StartAll(project.id)));
        }
        if is_any_active {
            actions.push(("Stop All", Action::StopAll(project.id)));
        }
        let services = project
            .services
            .iter()
            .map(|service| {
                let status = statuses.get(&service.id);
                let state = status.map_or(ServiceState::Stopped, |status| status.state);
                let is_live = matches!(state, ServiceState::Starting | ServiceState::Running);
                let reveal = Reveal::Project { project_id: project.id, service_id: Some(service.id) };
                let mut actions = vec![("Show in localhost", Action::Show(reveal))];
                if is_live && service.url_to_open(status.and_then(|status| status.run.as_ref())).is_some() {
                    actions.push(("Open in Browser", Action::OpenUrl(service.id)));
                }
                if is_live {
                    actions.push(("Restart", Action::Restart(service.id)));
                    actions.push(("Stop", Action::Stop(service.id)));
                } else if state != ServiceState::Stopping {
                    actions.push(("Start", Action::Start(service.id)));
                }
                ServiceRow { label: format!("{} {} — {}", glyph(state), service.name, summary(status)), actions }
            })
            .collect();
        let star = if project.is_favorite { " ★" } else { "" };
        ProjectSection { id: project.id, label: format!("{}{star}", project.name), actions, services }
    }
}

/// The state glyphs' shapes in text: a dot (Running), a dotted ring
/// (Starting, Stopping), a diamond (Failed), a ring (Stopped, Exited).
fn glyph(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Running => "●",
        ServiceState::Starting | ServiceState::Stopping => "◌",
        ServiceState::Failed => "◆",
        ServiceState::Stopped | ServiceState::Exited => "○",
    }
}

/// A short status, e.g. "Running · :3000" or "Failed · Command not found."
fn summary(status: Option<&ServiceStatus>) -> String {
    let Some(status) = status else { return "Stopped".into() };
    let run = status.run.as_ref();
    let text = match status.state {
        ServiceState::Stopped => "Stopped".into(),
        ServiceState::Starting => "Starting…".into(),
        ServiceState::Running => match run.and_then(|run| run.port) {
            Some(port) => format!("Running · :{port}"),
            None => "Running".into(),
        },
        ServiceState::Stopping => "Stopping…".into(),
        ServiceState::Exited => "Exited".into(),
        ServiceState::Failed => match run.and_then(|run| run.failure.as_ref()) {
            Some(failure) => format!("Failed · {}", LocalhostError::message(failure)),
            None => "Failed".into(),
        },
    };
    if text.chars().count() <= MAX_SUMMARY {
        return text;
    }
    let cut: String = text.chars().take(MAX_SUMMARY - 1).collect();
    format!("{}…", cut.trim_end())
}

/// e.g. "Port 3000 (My SaaS › Web) is held by python3 (PID 123)."
fn warning_text(warning: &PortWarning, projects: &[Project]) -> String {
    let owner = projects
        .iter()
        .find_map(|project| {
            project.service(warning.service_id).map(|service| format!("{} › {}", project.name, service.name))
        })
        .unwrap_or_else(|| "a service".into());
    let holder = &warning.holder;
    format!("Port {} ({owner}) is held by {} (PID {}).", warning.port, holder.name, holder.pid)
}

/// The menu items' actions by item id, for the menu's event handler.
type Actions = Arc<Mutex<HashMap<String, Action>>>;

/// Adds the tray icon, then keeps its menu current.
pub fn install(app: &App) -> tauri::Result<()> {
    let actions = Actions::default();
    let handler_actions = actions.clone();
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(false))
        .icon_as_template(true)
        .tooltip("localhost")
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let action = handler_actions.lock().unwrap().get(event.id().as_ref()).cloned();
            if let Some(action) = action {
                perform(app, action);
            }
        })
        .build(app)?;
    let handle = app.handle().clone();
    let menu = TrayMenu::default();
    render(&handle, &tray, &actions, &menu)?;

    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        let Ok(core) = state.core() else { return };
        let (_, mut library_events) = core.library.subscribe();
        let (_, mut service_events) = core.targets.subscribe();
        let mut scan = tokio::time::interval(LISTENER_SCAN);
        let mut warnings = Vec::new();
        let mut shown = menu;
        loop {
            tokio::select! {
                Some(_) = library_events.recv() => {}
                Some(_) = service_events.recv() => {}
                _ = scan.tick() => {
                    let listeners: Vec<_> = core.targets.listeners().await.into_iter().map(|(_, listener)| listener).collect();
                    let (projects, statuses) = current(core);
                    let statuses: Vec<ServiceStatus> = statuses.into_values().collect();
                    warnings = port_warnings(&projects, &statuses, &listeners);
                }
                else => return,
            }
            // Let a burst (Start All, a Run's state changes) settle first.
            tokio::time::sleep(SETTLE).await;
            while library_events.try_recv().is_ok() || service_events.try_recv().is_ok() {}
            let (projects, statuses) = current(core);
            let menu = TrayMenu::new(&projects, &statuses, &warnings);
            if menu != shown {
                if menu.has_failure != shown.has_failure {
                    let _ = tray.set_icon(Some(icon(menu.has_failure)));
                    let _ = tray.set_icon_as_template(!menu.has_failure);
                }
                let _ = render(&handle, &tray, &actions, &menu);
                shown = menu;
            }
        }
    });
    Ok(())
}

/// The Projects and the statuses of their Services, now.
fn current(core: &Core) -> (Vec<Project>, HashMap<Id, ServiceStatus>) {
    let projects = core.library.projects();
    let statuses = projects
        .iter()
        .flat_map(|project| &project.services)
        .map(|service| (service.id, core.targets.status(service.id)))
        .collect();
    (projects, statuses)
}

/// Builds the native menu for `menu`, replacing the items' actions.
fn render(app: &AppHandle, tray: &TrayIcon, actions: &Actions, menu: &TrayMenu) -> tauri::Result<()> {
    let mut by_id = HashMap::new();
    let mut item = |label: &str, action: Action| -> tauri::Result<MenuItem<Wry>> {
        let id = by_id.len().to_string();
        by_id.insert(id.clone(), action);
        MenuItem::with_id(app, id, escaped(label), true, None::<&str>)
    };
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for warning in &menu.warnings {
        items.push(Box::new(item(&format!("⚠ {warning}"), Action::Show(Reveal::Ports))?));
    }
    if !menu.warnings.is_empty() {
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    if menu.projects.is_empty() {
        items.push(Box::new(MenuItem::new(app, "No services running", false, None::<&str>)?));
        items.push(Box::new(MenuItem::new(app, "Favourite a project to keep it here.", false, None::<&str>)?));
    }
    for (index, project) in menu.projects.iter().enumerate() {
        if index > 0 {
            items.push(Box::new(PredefinedMenuItem::separator(app)?));
        }
        let submenu = Submenu::new(app, escaped(&project.label), true)?;
        for (label, action) in &project.actions {
            submenu.append(&item(label, action.clone())?)?;
        }
        items.push(Box::new(submenu));
        for service in &project.services {
            let submenu = Submenu::new(app, escaped(&service.label), true)?;
            for (label, action) in &service.actions {
                submenu.append(&item(label, action.clone())?)?;
            }
            items.push(Box::new(submenu));
        }
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(item("Open localhost", Action::OpenWindow)?));
    items.push(Box::new(item("Quit localhost", Action::Quit)?));

    let references: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|item| item.as_ref()).collect();
    let native = Menu::with_items(app, &references)?;
    *actions.lock().unwrap() = by_id;
    tray.set_menu(Some(native))
}

/// `&` marks a mnemonic in menu labels; a Project named "R&D" keeps its `&`.
fn escaped(label: &str) -> String {
    label.replace('&', "&&")
}

fn perform(app: &AppHandle, action: Action) {
    let origin = Origin::MenuBar;
    match action {
        Action::Show(reveal) => lifecycle::show_window(app, Some(reveal)),
        Action::OpenWindow => lifecycle::show_window(app, None),
        Action::Quit => lifecycle::quit(app, origin),
        action => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let Ok(core) = state.core() else { return };
                // A failure shows in the Service's status (a port conflict
                // Fails the start); there is nowhere else to say it here.
                let _ = match action {
                    Action::Start(id) => core.targets.start(id, None, origin).await.map(drop),
                    Action::Stop(id) => core.targets.stop(id, origin).await.map(drop),
                    Action::Restart(id) => core.targets.restart(id, origin).await.map(drop),
                    Action::StartAll(id) => core.targets.start_all(id, origin).await,
                    Action::StopAll(id) => core.targets.stop_all(id, origin).await,
                    Action::OpenUrl(id) => desktop::open_url_of(core, id).map_err(|error| error.error),
                    Action::Show(_) | Action::OpenWindow | Action::Quit => Ok(()),
                };
            });
        }
    }
}

/// The app icon's colon as a tray icon: white for the dark panels of GNOME
/// and most desktops (a template on macOS), with a red dot while a Service
/// is Failed.
fn icon(has_failure: bool) -> Image<'static> {
    const SIZE: usize = 32;
    const SAMPLES: usize = 4;
    // Two rounded squares, as in icon.svg, filling the height.
    let squares = [(10.5, 3.0), (10.5, 18.0)];
    let (side, radius) = (11.0, 3.4);
    let dot = (26.0, 6.0, 5.0);
    let in_square = |x: f64, y: f64| {
        squares.iter().any(|&(left, top)| {
            let (cx, cy) = (x.clamp(left + radius, left + side - radius), y.clamp(top + radius, top + side - radius));
            (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius && x >= left && y >= top
        })
    };
    let distance_to_dot = |x: f64, y: f64| ((x - dot.0).powi(2) + (y - dot.1).powi(2)).sqrt();
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for py in 0..SIZE {
        for px in 0..SIZE {
            let (mut white, mut red) = (0, 0);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f64 + (sx as f64 + 0.5) / SAMPLES as f64;
                    let y = py as f64 + (sy as f64 + 0.5) / SAMPLES as f64;
                    if has_failure && distance_to_dot(x, y) <= dot.2 {
                        red += 1;
                    } else if in_square(x, y) && !(has_failure && distance_to_dot(x, y) <= dot.2 + 1.5) {
                        white += 1;
                    }
                }
            }
            let total = (SAMPLES * SAMPLES) as u32;
            let (white, red) = (white as u32, red as u32);
            let pixel = &mut rgba[(py * SIZE + px) * 4..][..4];
            if white + red > 0 {
                // Premultiplied mix, then back to straight alpha.
                let alpha = (white + red) * 255 / total;
                let channel =
                    |white_value: u32, red_value: u32| ((white * white_value + red * red_value) / (white + red)) as u8;
                pixel.copy_from_slice(&[channel(255, 0xE5), channel(255, 0x48), channel(255, 0x4D), alpha as u8]);
            }
        }
    }
    Image::new_owned(rgba, SIZE as u32, SIZE as u32)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use localhost_core::{Key, MemorySecretStore, NewService, ProjectChanges, ProjectLibrary, Run};

    use super::*;

    /// Projects as the library makes them, in a throwaway folder.
    struct Library {
        library: ProjectLibrary,
        root: std::path::PathBuf,
    }

    impl Library {
        fn new() -> Library {
            let root = std::env::temp_dir().join(format!("localhost-tray-{}-{}", std::process::id(), Id::new()));
            std::fs::create_dir_all(&root).unwrap();
            let library = ProjectLibrary::open(&root, Arc::new(MemorySecretStore::new())).unwrap();
            Library { library, root }
        }

        fn add(&self, name: &str, services: &[&str]) -> Project {
            let key = |name: &str| Key::new(name.to_lowercase()).unwrap();
            let services = services.iter().map(|service| NewService::new(*service, key(service), "sleep 1")).collect();
            let path = self.root.to_string_lossy();
            self.library.add_project(name, key(name), &path, services, Origin::Cli).unwrap()
        }

        fn favourite(&self, project: &Project) -> Project {
            let changes = ProjectChanges { is_favorite: Some(true), ..Default::default() };
            self.library.edit_project(project.id, changes, Origin::Cli).unwrap()
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn status(service_id: Id, state: ServiceState, failure: Option<LocalhostError>) -> (Id, ServiceStatus) {
        let run = Run {
            id: Id::new(),
            service_id,
            origin: Origin::MainWindow,
            started_at: SystemTime::now(),
            ended_at: None,
            termination: None,
            failure,
            stop_origin: None,
            was_force_killed: false,
            port: None,
            port_change: None,
            is_port_unresponsive: false,
            detected_url: None,
        };
        let status = ServiceStatus {
            service_id,
            state,
            run: Some(run),
            awaited_dependencies: Vec::new(),
            is_restart_needed: false,
        };
        (service_id, status)
    }

    fn titles(actions: &[(&'static str, Action)]) -> Vec<&'static str> {
        actions.iter().map(|(title, _)| *title).collect()
    }

    #[test]
    fn shows_projects_with_an_active_or_failed_service_and_favourites() {
        let library = Library::new();
        let shop = library.add("Shop", &["Web", "API"]);
        let game = library.add("Game", &["Server"]);
        let docs = library.favourite(&library.add("Docs", &["Site"]));
        let statuses = HashMap::from([status(shop.services[0].id, ServiceState::Running, None)]);

        let menu = TrayMenu::new(&[shop.clone(), game, docs], &statuses, &[]);

        let labels: Vec<&str> = menu.projects.iter().map(|project| project.label.as_str()).collect();
        assert_eq!(labels, ["Shop", "Docs ★"]);
        assert!(!menu.has_failure);
        assert_eq!(titles(&menu.projects[0].actions), ["Show in localhost", "Start All", "Stop All"]);
        let web = &menu.projects[0].services[0];
        assert_eq!(web.label, "● Web — Running");
        assert_eq!(titles(&web.actions), ["Show in localhost", "Restart", "Stop"]);
        let api = &menu.projects[0].services[1];
        assert_eq!(api.label, "○ API — Stopped");
        assert_eq!(api.actions[1], ("Start", Action::Start(shop.services[1].id)));
        assert_eq!(titles(&menu.projects[1].actions), ["Show in localhost", "Start All"], "nothing to stop");
    }

    #[test]
    fn a_failed_service_says_why_and_marks_the_icon() {
        let library = Library::new();
        let shop = library.add("Shop", &["Web"]);
        let web = shop.services[0].id;
        let statuses = HashMap::from([status(web, ServiceState::Failed, Some(LocalhostError::CommandNotFound))]);

        let menu = TrayMenu::new(&[shop], &statuses, &[]);

        assert!(menu.has_failure);
        let label = &menu.projects[0].services[0].label;
        assert_eq!(*label, format!("◆ Web — Failed · {}", LocalhostError::CommandNotFound.message()));
    }

    #[test]
    fn long_summaries_are_cut() {
        let failure = LocalhostError::ShellNotFound("x".repeat(200));
        let (_, failed) = status(Id::new(), ServiceState::Failed, Some(failure));
        let text = summary(Some(&failed));
        assert_eq!(text.chars().count(), MAX_SUMMARY);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn labels_keep_their_ampersands() {
        assert_eq!(escaped("R&D"), "R&&D");
    }

    #[test]
    fn the_failure_icon_has_a_red_dot() {
        let (plain, failed) = (icon(false), icon(true));
        let pixel = |image: &Image, x: usize, y: usize| image.rgba()[(y * 32 + x) * 4..][..4].to_vec();
        assert_eq!(pixel(&plain, 16, 8), [255, 255, 255, 255], "the top square");
        assert_eq!(pixel(&plain, 26, 6)[3], 0, "no dot");
        assert_eq!(pixel(&failed, 26, 6), [0xE5, 0x48, 0x4D, 255]);
        assert_eq!(pixel(&failed, 16, 24), pixel(&plain, 16, 24), "the bottom square is untouched");
    }
}
