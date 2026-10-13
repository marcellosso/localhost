use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::platform::{self, ProcessStartTime};
use crate::process::{StopPolicy, process_group_exists};
use crate::{Id, LocalhostError, Project, ServiceStatus, service_label};

/// A process listening on one or more TCP ports on this machine, whoever
/// started it (see `GLOSSARY.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Listener {
    pub pid: i32,
    pub process_group_id: i32,
    /// The executable's name, e.g. `node`.
    pub name: String,
    /// The full command line, e.g. `python3 -m http.server 3000`.
    pub command: String,
    /// The executable's path, if it could be read.
    pub executable_path: Option<String>,
    /// The TCP ports it listens on, ascending.
    pub ports: Vec<u16>,
    /// Its current working directory, if it could be read.
    pub working_directory: Option<String>,
    pub owner: ListenerOwner,
    /// The label of the launchd job it belongs to, if any (e.g. a Homebrew
    /// service or an app). localhost never stops those.
    pub launchd_label: Option<String>,
    pub kind: ListenerKind,
    pub(crate) start_time: ProcessStartTime,
}

impl Listener {
    /// Whether it belongs to no Run and no Project.
    pub fn is_unmanaged(&self) -> bool {
        self.owner == ListenerOwner::Unmanaged
    }

    /// Why localhost refuses to stop this Listener, or `None` if it can:
    /// localhost itself, launchd jobs and Docker's backend.
    pub fn reason_not_stoppable(&self) -> Option<LocalhostError> {
        if self.pid == std::process::id() as i32 {
            Some(LocalhostError::ListenerIsLocalhost)
        } else if let Some(label) = &self.launchd_label {
            Some(LocalhostError::ListenerIsLaunchdJob { label: label.clone() })
        } else if self.kind == ListenerKind::Docker {
            Some(LocalhostError::ListenerIsDocker)
        } else {
            None
        }
    }

    /// Who it belongs to, e.g. "My SaaS › API", "In My SaaS", "Docker", "Unmanaged".
    pub fn owner_description(&self, projects: &[Project]) -> String {
        match self.owner {
            ListenerOwner::Run { service_id, .. } => {
                service_label(projects, service_id).unwrap_or_else(|| "A removed service".into())
            }
            ListenerOwner::Project { project_id } => {
                let project = projects.iter().find(|project| project.id == project_id);
                format!("In {}", project.map_or("a removed project", |project| project.name.as_str()))
            }
            ListenerOwner::Unmanaged => match self.kind {
                ListenerKind::Docker => "Docker".into(),
                ListenerKind::App => "App".into(),
                ListenerKind::System if cfg!(target_os = "macos") => "macOS".into(),
                ListenerKind::System => "System".into(),
                ListenerKind::Development => "Unmanaged".into(),
            },
        }
    }
}

/// A configured Port held by an Unmanaged Listener, so its Service can't
/// start on it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortWarning {
    pub port: u16,
    pub service_id: Id,
    pub holder: Listener,
}

/// The Port warnings for `listeners`, in Project and Service order. An active
/// Service is left out: its Run already says why it moved or failed.
pub fn port_warnings(projects: &[Project], statuses: &[ServiceStatus], listeners: &[Listener]) -> Vec<PortWarning> {
    let is_active =
        |service_id: Id| statuses.iter().any(|status| status.service_id == service_id && status.state.is_active());
    projects
        .iter()
        .flat_map(|project| &project.services)
        .filter_map(|service| {
            let port = service.port?;
            if is_active(service.id) {
                return None;
            }
            let holder = listeners.iter().find(|listener| listener.is_unmanaged() && listener.ports.contains(&port))?;
            Some(PortWarning { port, service_id: service.id, holder: holder.clone() })
        })
        .collect()
}

/// Who a Listener is attributed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ListenerOwner {
    /// A process of a Run localhost started.
    #[serde(rename_all = "camelCase")]
    Run {
        service_id: Id,
        run_id: Id,
    },
    /// Not started by localhost, but its working directory is inside the Project's folder.
    Project {
        project_id: Id,
    },
    Unmanaged,
}

/// What kind of program a Listener is, so development servers can be told
/// apart from the apps and system services that also listen on ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ListenerKind {
    /// A development server or tool: anything not recognised as below, and
    /// always a Listener of a Run or inside a Project.
    Development,
    /// Docker's backend, which listens on the ports of running containers.
    Docker,
    /// A desktop app (or a helper inside one).
    App,
    /// Part of the operating system.
    System,
}

impl ListenerKind {
    /// Classifies by owner, executable path and launchd label.
    pub fn of(executable_path: Option<&str>, launchd_label: Option<&str>, owner: ListenerOwner) -> ListenerKind {
        if owner != ListenerOwner::Unmanaged {
            return ListenerKind::Development;
        }
        let path = executable_path.unwrap_or_default();
        let file_name = path.rsplit('/').next().unwrap_or_default();
        if cfg!(target_os = "macos") {
            if path.contains("/Docker.app/") && file_name.starts_with("com.docker.") {
                return ListenerKind::Docker;
            }
            let system_folders = ["/System/", "/usr/libexec/", "/usr/sbin/", "/sbin/", "/Library/Apple/"];
            if system_folders.iter().any(|folder| path.starts_with(folder))
                || launchd_label.is_some_and(|label| label.starts_with("com.apple."))
            {
                return ListenerKind::System;
            }
            // Interpreters like Homebrew's Python run from an `.app` inside their framework.
            if (path.contains(".app/") && !path.contains(".framework/"))
                || launchd_label.is_some_and(|label| label.starts_with("application."))
            {
                return ListenerKind::App;
            }
        } else {
            // Docker Desktop's backend, or rootless Docker publishing a container's port.
            if file_name.starts_with("com.docker.") || ["docker-proxy", "rootlesskit"].contains(&file_name) {
                return ListenerKind::Docker;
            }
            let system_folders = ["/usr/sbin/", "/sbin/", "/usr/libexec/", "/usr/lib/systemd/", "/lib/systemd/"];
            if system_folders.iter().any(|folder| path.starts_with(folder)) {
                return ListenerKind::System;
            }
            // Snap and Flatpak apps, and mounted AppImages.
            let app_folders = ["/snap/", "/var/lib/flatpak/", "/app/", "/tmp/.mount_"];
            if app_folders.iter().any(|folder| path.starts_with(folder)) {
                return ListenerKind::App;
            }
        }
        ListenerKind::Development
    }
}

/// Every Listener on this machine, by first port, before attribution: the
/// raw processes plus the managed jobs they belong to. Blocking.
pub(crate) fn scan() -> (Vec<platform::ListeningProcess>, HashMap<i32, String>) {
    (platform::scan_listeners(), platform::managed_jobs())
}

/// Stops a Listener localhost didn't start: SIGINT, then SIGTERM, then SIGKILL.
///
/// Signals the Listener's process group, unless the group's leader is a
/// session leader (e.g. a terminal's shell) or the group is localhost's own:
/// then only the Listener's process and its descendants.
pub(crate) async fn stop_listener(listener: &Listener, policy: StopPolicy) {
    let group = listener.process_group_id;
    let is_own_group = group == unsafe { libc::getpgrp() };
    let leads_session = unsafe { libc::getsid(group) } == group;
    let mut target = if group > 1 && !is_own_group && !leads_session {
        Target::Group(group)
    } else {
        Target::Processes(HashMap::from([(listener.pid, listener.start_time)]))
    };
    let steps = [
        (libc::SIGINT, policy.interrupt_timeout),
        (libc::SIGTERM, policy.terminate_timeout),
        (libc::SIGKILL, Duration::from_secs(5)),
    ];
    for (signal, timeout) in steps {
        target.include_descendants();
        if !target.is_alive() {
            return;
        }
        target.send(signal);
        let deadline = Instant::now() + timeout;
        while target.is_alive() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

enum Target {
    Group(i32),
    /// Processes identified by pid and start time, so a reused pid is never signalled.
    Processes(HashMap<i32, ProcessStartTime>),
}

impl Target {
    fn is_alive(&self) -> bool {
        match self {
            Target::Group(group) => process_group_exists(*group),
            Target::Processes(processes) => {
                processes.iter().any(|(&pid, start_time)| ProcessStartTime::of(pid) == Some(*start_time))
            }
        }
    }

    fn send(&self, signal: i32) {
        match self {
            Target::Group(group) => unsafe {
                libc::kill(-group, signal);
            },
            Target::Processes(processes) => {
                for (&pid, start_time) in processes {
                    if ProcessStartTime::of(pid) == Some(*start_time) {
                        unsafe { libc::kill(pid, signal) };
                    }
                }
            }
        }
    }

    /// Adds the current descendants of the tracked processes that are still alive.
    fn include_descendants(&mut self) {
        let Target::Processes(processes) = self else { return };
        let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
        for (pid, parent) in platform::parents() {
            children.entry(parent).or_default().push(pid);
        }
        let mut queue: Vec<i32> = processes
            .iter()
            .filter(|(pid, start_time)| ProcessStartTime::of(**pid) == Some(**start_time))
            .map(|(pid, _)| *pid)
            .collect();
        while let Some(pid) = queue.pop() {
            for &child in children.get(&pid).into_iter().flatten() {
                if processes.contains_key(&child) {
                    continue;
                }
                let Some(start_time) = ProcessStartTime::of(child) else { continue };
                processes.insert(child, start_time);
                queue.push(child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LaunchConfiguration, ProcessTree};

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn quick_stop() -> StopPolicy {
        StopPolicy { interrupt_timeout: Duration::from_millis(500), terminate_timeout: Duration::from_millis(500) }
    }

    /// Starts a process tree that listens on `port`, and waits for its Listener.
    async fn listen(port: u16, command_line: &str) -> (ProcessTree, Listener) {
        let configuration = LaunchConfiguration {
            command_line: command_line.into(),
            working_directory: std::env::temp_dir(),
            environment: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
            shell: "/bin/sh".into(),
        };
        let tree = ProcessTree::launch(&configuration, quick_stop()).unwrap();
        loop {
            let (processes, _) = scan();
            if let Some(process) = processes.into_iter().find(|process| process.ports.contains(&port)) {
                let listener = Listener {
                    pid: process.pid,
                    process_group_id: process.process_group_id,
                    name: process.name,
                    command: process.command,
                    executable_path: process.executable_path,
                    ports: process.ports,
                    working_directory: process.working_directory,
                    owner: ListenerOwner::Unmanaged,
                    launchd_label: None,
                    kind: ListenerKind::Development,
                    start_time: process.start_time,
                };
                return (tree, listener);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn discovers_a_listeners_command_and_ports() {
        let port = free_port();
        let (tree, listener) = listen(port, &format!("exec nc -lk {port}")).await;
        assert_eq!(listener.pid, tree.pid());
        assert_eq!(listener.name, "nc");
        assert!(
            listener.command.starts_with("nc") && listener.command.ends_with(&format!("-lk {port}")),
            "{}",
            listener.command
        );
        assert_eq!(listener.ports, [port]);
        assert!(listener.executable_path.is_some_and(|path| path.contains("nc")));
        tree.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stops_a_listeners_own_process_group() {
        let port = free_port();
        // nc leads a process group of its own, inside the tree's session. (Not
        // `set -m`: dash, Linux's sh, turns job control off without a terminal.)
        let (tree, listener) = listen(port, &format!("perl -e 'setpgrp(0, 0); exec @ARGV' nc -lk {port} & wait")).await;
        assert_eq!(listener.process_group_id, listener.pid);
        assert_ne!(listener.pid, tree.pid());

        stop_listener(&listener, quick_stop()).await;

        assert!(!crate::port_probe::is_accepting(port).await);
        assert!(ProcessStartTime::of(listener.pid).is_none());
        tree.wait_for_exit().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stops_a_session_leader_listener_and_its_descendants_not_its_whole_session() {
        let port = free_port();
        // nc leads its session and group; the sleep it inherits is its child.
        let (tree, listener) = listen(port, &format!("sleep 1000 & exec nc -lk {port}")).await;
        assert_eq!(listener.pid, tree.pid());

        stop_listener(&listener, quick_stop()).await;

        // The tree has ended only once the sleep is gone too.
        tree.wait_for_exit().await;
        assert!(!crate::port_probe::is_accepting(port).await);
    }

    fn kind(path: &str, label: Option<&str>) -> ListenerKind {
        ListenerKind::of(Some(path), label, ListenerOwner::Unmanaged)
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn tells_development_servers_from_apps_and_macos() {
        use ListenerKind::*;
        for (path, label, expected) in [
            ("/opt/homebrew/Cellar/node/22.1.0/bin/node", None, Development),
            (
                "/opt/homebrew/Cellar/python@3.13/3.13.1/Frameworks/Python.framework/Versions/3.13/Resources/Python.app/Contents/MacOS/Python",
                None,
                Development,
            ),
            (
                "/Applications/Xcode.app/Contents/Developer/Library/Frameworks/Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python",
                None,
                Development,
            ),
            ("/usr/bin/nc", None, Development),
            ("/Applications/Docker.app/Contents/MacOS/com.docker.backend", None, Docker),
            ("/Applications/Spotify.app/Contents/MacOS/Spotify", Some("application.com.spotify.client.1.2"), App),
            (
                "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Plugin).app/Contents/MacOS/Cursor Helper (Plugin)",
                None,
                App,
            ),
            (
                "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter",
                Some("com.apple.controlcenter"),
                System,
            ),
            ("/usr/libexec/rapportd", None, System),
        ] {
            assert_eq!(kind(path, label), expected, "{path}");
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn tells_development_servers_from_apps_and_the_system() {
        use ListenerKind::*;
        for (path, expected) in [
            ("/home/dev/.nvm/versions/node/v22.1.0/bin/node", Development),
            ("/usr/bin/python3.12", Development),
            ("/opt/docker-desktop/bin/com.docker.backend", Docker),
            ("/usr/bin/rootlesskit", Docker),
            ("/usr/sbin/cupsd", System),
            ("/usr/lib/systemd/systemd-resolved", System),
            ("/snap/spotify/80/usr/share/spotify/spotify", App),
        ] {
            assert_eq!(kind(path, None), expected, "{path}");
        }
    }

    #[test]
    fn a_listener_of_a_run_or_a_project_is_always_development() {
        let app = Some("/Applications/Some.app/Contents/MacOS/Some");
        let project = ListenerOwner::Project { project_id: Id::new() };
        let run = ListenerOwner::Run { service_id: Id::new(), run_id: Id::new() };
        assert_eq!(ListenerKind::of(app, None, project), ListenerKind::Development);
        assert_eq!(ListenerKind::of(app, None, run), ListenerKind::Development);
    }

    fn listener(owner: ListenerOwner, kind: ListenerKind, ports: &[u16]) -> Listener {
        Listener {
            pid: 999_999,
            process_group_id: 999_999,
            name: "node".into(),
            command: "node server.js".into(),
            executable_path: None,
            ports: ports.to_vec(),
            working_directory: None,
            owner,
            launchd_label: None,
            kind,
            start_time: ProcessStartTime { seconds: 0, microseconds: 0 },
        }
    }

    #[test]
    fn says_why_a_listener_cant_be_stopped() {
        let mut ours = listener(ListenerOwner::Unmanaged, ListenerKind::App, &[1]);
        ours.pid = std::process::id() as i32;
        assert_eq!(ours.reason_not_stoppable(), Some(LocalhostError::ListenerIsLocalhost));

        let mut job = listener(ListenerOwner::Unmanaged, ListenerKind::App, &[1]);
        job.launchd_label = Some("homebrew.mxcl.postgresql".into());
        assert!(matches!(job.reason_not_stoppable(), Some(LocalhostError::ListenerIsLaunchdJob { .. })));

        let docker = listener(ListenerOwner::Unmanaged, ListenerKind::Docker, &[1]);
        assert_eq!(docker.reason_not_stoppable(), Some(LocalhostError::ListenerIsDocker));

        let server = listener(ListenerOwner::Unmanaged, ListenerKind::Development, &[1]);
        assert_eq!(server.reason_not_stoppable(), None);
    }

    #[test]
    fn describes_who_a_listener_belongs_to() {
        let projects = [Project::with_services("My SaaS", &[("API", None)])];
        let (project_id, service_id) = (projects[0].id, projects[0].services[0].id);
        let describe = |owner, kind| listener(owner, kind, &[1]).owner_description(&projects);
        use ListenerKind::*;

        assert_eq!(describe(ListenerOwner::Run { service_id, run_id: Id::new() }, Development), "My SaaS › API");
        assert_eq!(
            describe(ListenerOwner::Run { service_id: Id::new(), run_id: Id::new() }, Development),
            "A removed service"
        );
        assert_eq!(describe(ListenerOwner::Project { project_id }, Development), "In My SaaS");
        assert_eq!(describe(ListenerOwner::Project { project_id: Id::new() }, Development), "In a removed project");
        assert_eq!(describe(ListenerOwner::Unmanaged, Docker), "Docker");
        assert_eq!(describe(ListenerOwner::Unmanaged, Development), "Unmanaged");
        let system = if cfg!(target_os = "macos") { "macOS" } else { "System" };
        assert_eq!(describe(ListenerOwner::Unmanaged, System), system);
    }

    #[test]
    fn warns_about_ports_unmanaged_listeners_hold() {
        let projects =
            [Project::with_services("My SaaS", &[("API", Some(4000)), ("Web", Some(3000)), ("Worker", None)])];
        let (api, web) = (projects[0].services[0].id, projects[0].services[1].id);
        let held = listener(ListenerOwner::Unmanaged, ListenerKind::Development, &[3000, 4000]);
        let ours =
            listener(ListenerOwner::Run { service_id: api, run_id: Id::new() }, ListenerKind::Development, &[4000]);

        let warnings = port_warnings(&projects, &[], &[ours.clone(), held.clone()]);
        let found: Vec<_> = warnings.iter().map(|warning| (warning.service_id, warning.port)).collect();
        assert_eq!(found, [(api, 4000), (web, 3000)]);
        assert!(warnings.iter().all(|warning| warning.holder == held));

        // An active Service's own Run explains it.
        let mut running = ServiceStatus::never_run(web);
        running.state = crate::ServiceState::Running;
        let warnings = port_warnings(&projects, &[running], &[held]);
        assert_eq!(warnings.iter().map(|warning| warning.service_id).collect::<Vec<_>>(), [api]);
        assert!(port_warnings(&projects, &[], &[ours]).is_empty());
    }
}
