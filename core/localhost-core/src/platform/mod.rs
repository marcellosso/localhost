//! What the operating system tells about processes: who is listening on
//! which TCP port, when a process started, its parent, what it uses, and
//! signalling them.
//! One implementation per platform; everything above this module is shared.
//!
//! On Windows there are no process groups or signals: Runs live in Job
//! Objects (ADR-0011), and the signals here stand for what localhost does
//! instead (`windows.rs`). Pids and signals of WSL Projects come from the
//! distro's Linux (ADR-0008).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(unix)]
use unix as groups;
#[cfg(windows)]
use windows as groups;
#[cfg(windows)]
use windows as imp;

/// A process id: `pid_t` on every Unix localhost runs on; a Windows process
/// id (a `DWORD`, always small enough) on Windows.
pub type Pid = i32;

/// The signals localhost sends or names. On Windows they have Linux's
/// numbers, as in WSL distros; natively SIGINT stands for Ctrl-Break and the
/// others for terminating.
pub mod signal {
    #[cfg(unix)]
    pub use libc::{SIGABRT, SIGBUS, SIGHUP, SIGINT, SIGKILL, SIGPIPE, SIGQUIT, SIGSEGV, SIGTERM};

    #[cfg(not(unix))]
    pub const SIGHUP: i32 = 1;
    #[cfg(not(unix))]
    pub const SIGINT: i32 = 2;
    #[cfg(not(unix))]
    pub const SIGQUIT: i32 = 3;
    #[cfg(not(unix))]
    pub const SIGABRT: i32 = 6;
    #[cfg(not(unix))]
    pub const SIGBUS: i32 = 7;
    #[cfg(not(unix))]
    pub const SIGKILL: i32 = 9;
    #[cfg(not(unix))]
    pub const SIGSEGV: i32 = 11;
    #[cfg(not(unix))]
    pub const SIGPIPE: i32 = 13;
    #[cfg(not(unix))]
    pub const SIGTERM: i32 = 15;
}

/// When a process started, as the kernel reports it. Together with a pid it
/// identifies one process, even after the pid has been reused.
///
/// On macOS it is the wall-clock start time;
/// on Linux, the time since boot. Only equality matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ProcessStartTime {
    pub seconds: u64,
    pub microseconds: u64,
}

impl ProcessStartTime {
    /// The start time of a live process, `None` if there is none with that pid.
    pub fn of(pid: Pid) -> Option<ProcessStartTime> {
        imp::start_time(pid)
    }
}

/// A process listening on TCP ports, as the kernel reports it.
#[derive(Clone, Debug)]
pub(crate) struct ListeningProcess {
    pub pid: Pid,
    /// On Windows the pid itself: localhost's "group" is a process and its descendants.
    pub process_group_id: Pid,
    #[cfg(unix)]
    pub session_id: Pid,
    pub name: String,
    pub command: String,
    pub executable_path: Option<String>,
    /// Ascending, without duplicates.
    pub ports: Vec<u16>,
    pub working_directory: Option<String>,
    pub start_time: ProcessStartTime,
}

/// Every process of this user listening on a TCP port. Processes of other
/// users (e.g. root) can't be inspected and are left out. Blocking.
pub(crate) fn scan_listeners() -> Vec<ListeningProcess> {
    imp::scan_listeners()
}

/// The parent of every process, for finding descendants. Blocking.
pub(crate) fn parents() -> HashMap<Pid, Pid> {
    imp::parents()
}

/// What one process uses, as the kernel reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessUsage {
    /// User and system CPU time of the process. On macOS and Linux it
    /// includes the children it has waited for, so the time of a process
    /// that ended in a Run's tree isn't lost once its parent reaps it.
    pub cpu_time: Duration,
    /// Its memory, counting shared pages once where the platform can: the
    /// physical footprint on macOS (Activity Monitor's "Memory"), the
    /// proportional set size on Linux (the resident set where that can't be
    /// read), private bytes on Windows (ADR-0012).
    pub memory_bytes: u64,
}

/// What a live process of this user uses, `None` if there is none with that
/// pid (or it has ended and waits to be reaped). Blocking.
pub(crate) fn usage(pid: Pid) -> Option<ProcessUsage> {
    imp::usage(pid)
}

/// Where a process sits among the others.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessRelations {
    pub pid: Pid,
    pub parent: Pid,
    pub process_group: Pid,
    pub session: Pid,
}

/// The relations of every live process; zombies are left out. Blocking.
#[cfg(unix)]
pub(crate) fn relations() -> Vec<ProcessRelations> {
    imp::relations()
}

/// The live processes of the tree a session leader started: those in its
/// session or its process group, and its descendants, which may have left
/// both with `setsid` (ADR-0012). Includes the leader, if alive.
#[cfg(unix)]
pub(crate) fn tree_members(leader: Pid, relations: &[ProcessRelations]) -> Vec<Pid> {
    let mut members: Vec<Pid> = relations
        .iter()
        .filter(|process| process.pid == leader || process.session == leader || process.process_group == leader)
        .map(|process| process.pid)
        .collect();
    let mut index = 0;
    while index < members.len() {
        let parent = members[index];
        index += 1;
        for process in relations {
            if process.parent == parent && process.pid != parent && !members.contains(&process.pid) {
                members.push(process.pid);
            }
        }
    }
    members
}

/// The pids of the user's running service-manager jobs (launchd on macOS),
/// by label. localhost never stops those. Blocking.
pub(crate) fn managed_jobs() -> HashMap<Pid, String> {
    imp::managed_jobs()
}

/// Whether any process is left in the group (a zombie still counts).
pub(crate) fn process_group_exists(group: Pid) -> bool {
    groups::process_group_exists(group)
}

pub(crate) fn signal_process_group(group: Pid, signal: i32) {
    groups::signal_process_group(group, signal)
}

pub(crate) fn signal_process(pid: Pid, signal: i32) {
    groups::signal_process(pid, signal)
}

pub(crate) fn own_process_group() -> Pid {
    groups::own_process_group()
}

/// Whether the group's leader leads its session, e.g. a terminal's shell.
pub(crate) fn leads_session(group: Pid) -> bool {
    groups::leads_session(group)
}

/// `path` with symlinks resolved, as the kernel reports working directories.
/// On Windows without the `\\?\` prefix canonicalizing adds, as a process's
/// working directory has none.
pub(crate) fn resolved_path(path: &str) -> String {
    let resolved =
        std::fs::canonicalize(path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string());
    if cfg!(windows)
        && let Some(drive_path) = resolved.strip_prefix(r"\\?\").filter(|rest| rest.get(1..2) == Some(":"))
    {
        return drive_path.to_string();
    }
    resolved
}

/// Whether the working directory `directory` is `folder` or inside it. On
/// Windows paths compare case-insensitively, with either separator.
pub(crate) fn is_inside(directory: &str, folder: &str) -> bool {
    is_inside_on(directory, folder, cfg!(windows))
}

fn is_inside_on(directory: &str, folder: &str, windows: bool) -> bool {
    if windows {
        let comparable = |path: &str| path.replace('/', "\\").trim_end_matches('\\').to_lowercase();
        let (directory, folder) = (comparable(directory), comparable(folder));
        directory == folder || directory.starts_with(&format!("{folder}\\"))
    } else {
        directory == folder || directory.starts_with(&format!("{}/", folder.trim_end_matches('/')))
    }
}

fn normalized(mut ports: Vec<u16>) -> Vec<u16> {
    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_whether_a_directory_is_inside_a_folder() {
        assert!(is_inside_on("/work/app", "/work/app", false));
        assert!(is_inside_on("/work/app/web", "/work/app/", false));
        assert!(!is_inside_on("/work/application", "/work/app", false));
        assert!(!is_inside_on("/WORK/app", "/work/app", false));

        assert!(is_inside_on(r"C:\Work\App", r"c:\work\app\", true));
        assert!(is_inside_on(r"C:\Work\App\web", "C:/Work/App", true));
        assert!(!is_inside_on(r"C:\Work\Application", r"C:\Work\App", true));
        assert!(is_inside_on(r"C:\Work", r"C:\", true));
    }

    #[test]
    #[cfg(unix)]
    fn reads_a_processs_start_time() {
        let mine = ProcessStartTime::of(std::process::id() as i32).unwrap();
        let parent = ProcessStartTime::of(unsafe { libc::getppid() }).unwrap();
        assert_ne!(parent, mine);
        assert_eq!(ProcessStartTime::of(99_999_999), None);
    }

    #[test]
    fn finds_a_listening_socket_of_this_process() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let me = std::process::id() as i32;
        let found = scan_listeners().into_iter().find(|process| process.pid == me).expect("this process listens");
        assert!(found.ports.contains(&port), "{:?}", found.ports);
        assert_eq!(found.start_time, ProcessStartTime::of(me).unwrap());
        assert!(found.executable_path.is_some());
        assert_eq!(found.working_directory.as_deref(), Some(resolved_path(".").as_str()));
    }

    #[test]
    #[cfg(unix)]
    fn finds_a_trees_members_by_session_group_and_parent() {
        let process = |pid, parent, process_group, session| ProcessRelations { pid, parent, process_group, session };
        let relations = [
            process(1, 0, 1, 1),
            // The Run's shell leads its session and group.
            process(10, 1, 10, 10),
            process(11, 10, 10, 10),
            // Left the group but not the session.
            process(12, 11, 12, 10),
            // Left both with setsid, and its child.
            process(13, 11, 13, 13),
            process(14, 13, 13, 13),
            // Reparented to init but still in the group.
            process(15, 1, 10, 10),
            // Someone else's.
            process(20, 1, 20, 20),
            process(21, 20, 20, 20),
        ];
        let mut members = tree_members(10, &relations);
        members.sort();
        assert_eq!(members, [10, 11, 12, 13, 14, 15]);
        assert_eq!(tree_members(99, &relations), Vec::<Pid>::new());
    }

    #[test]
    fn reads_what_this_process_uses() {
        let me = std::process::id() as i32;
        let before = usage(me).expect("this process is alive");
        // Burn some CPU time.
        let started = std::time::Instant::now();
        let mut counter = 0u64;
        while started.elapsed() < Duration::from_millis(100) {
            counter = std::hint::black_box(counter.wrapping_add(1));
        }
        let after = usage(me).unwrap();
        assert!(after.cpu_time > before.cpu_time, "{before:?} → {after:?}");
        assert!(after.memory_bytes > 1024 * 1024, "{after:?}");
        assert_eq!(usage(99_999_999), None);
    }

    #[test]
    #[cfg(unix)]
    fn knows_this_processs_relations() {
        let me = std::process::id() as i32;
        let mine = relations().into_iter().find(|process| process.pid == me).expect("this process is listed");
        assert_eq!(mine.parent, unsafe { libc::getppid() });
        assert_eq!(mine.process_group, unsafe { libc::getpgrp() });
        assert_eq!(mine.session, unsafe { libc::getsid(0) });
    }

    #[test]
    #[cfg(unix)]
    fn knows_this_processs_parent() {
        let me = std::process::id() as i32;
        assert_eq!(parents().get(&me), Some(&unsafe { libc::getppid() }));
    }
}
