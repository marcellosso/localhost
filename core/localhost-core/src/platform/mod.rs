//! What the operating system tells about processes: who is listening on
//! which TCP port, when a process started, its parent. One implementation
//! per platform; everything above this module is shared.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;

/// When a process started, as the kernel reports it. Together with a pid it
/// identifies one process, even after the pid has been reused.
///
/// On macOS it is the wall-clock start time (as the Swift core recorded it);
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
    pub fn of(pid: libc::pid_t) -> Option<ProcessStartTime> {
        imp::start_time(pid)
    }
}

/// A process listening on TCP ports, as the kernel reports it.
#[derive(Clone, Debug)]
pub(crate) struct ListeningProcess {
    pub pid: libc::pid_t,
    pub process_group_id: libc::pid_t,
    pub session_id: libc::pid_t,
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
pub(crate) fn parents() -> HashMap<libc::pid_t, libc::pid_t> {
    imp::parents()
}

/// The pids of the user's running service-manager jobs (launchd on macOS),
/// by label. localhost never stops those. Blocking.
pub(crate) fn managed_jobs() -> HashMap<libc::pid_t, String> {
    imp::managed_jobs()
}

/// `path` with symlinks resolved, as the kernel reports working directories.
pub(crate) fn resolved_path(path: &str) -> String {
    std::fs::canonicalize(path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string())
}

/// Sorts and deduplicates ports.
fn normalized(mut ports: Vec<u16>) -> Vec<u16> {
    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
    fn knows_this_processs_parent() {
        let me = std::process::id() as i32;
        assert_eq!(parents().get(&me), Some(&unsafe { libc::getppid() }));
    }
}
