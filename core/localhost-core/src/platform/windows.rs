//! Processes on Windows. Runs live in Job Objects (`process::windows`); what
//! is here serves everything else that names a process by its pid: Orphans
//! and Listeners. Windows has no process-group signals and Ctrl-Break only
//! reaches processes on localhost's own console, so a "group" here is a
//! process and its descendants, and only terminating them stops them
//! (`docs/cross-platform.md`, Native Windows).
//!
//! Listener discovery (XP5) fills in `scan_listeners` and `managed_jobs`.

use std::collections::HashMap;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess,
};

use super::signal::SIGINT;
use super::{ListeningProcess, Pid, ProcessStartTime};

/// 100-nanosecond intervals from 1601-01-01 (`FILETIME`'s epoch) to 1970-01-01.
const UNIX_EPOCH_IN_FILETIME: u64 = 116_444_736_000_000_000;
/// The exit code of processes localhost terminates.
const TERMINATED_EXIT_CODE: u32 = 1;

fn open(pid: Pid, access: u32) -> Option<OwnedHandle> {
    let handle = unsafe { OpenProcess(access, 0, pid as u32) };
    (!handle.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// The process's creation time, as wall-clock time.
pub(super) fn start_time(pid: Pid) -> Option<ProcessStartTime> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
    if unsafe { GetProcessTimes(process.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let ticks =
        ((creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64).saturating_sub(UNIX_EPOCH_IN_FILETIME);
    Some(ProcessStartTime { seconds: ticks / 10_000_000, microseconds: ticks % 10_000_000 / 10 })
}

pub(super) fn scan_listeners() -> Vec<ListeningProcess> {
    Vec::new()
}

/// The parent of every live process, from a Toolhelp snapshot. A parent may
/// have ended (Windows doesn't reparent), so its pid may be reused: callers
/// pair this with start times where that matters.
pub(super) fn parents() -> HashMap<Pid, Pid> {
    let mut parents = HashMap::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return parents;
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) } != 0;
    while found {
        parents.insert(entry.th32ProcessID as Pid, entry.th32ParentProcessID as Pid);
        found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    parents
}

pub(super) fn managed_jobs() -> HashMap<Pid, String> {
    HashMap::new()
}

/// The pid and every live descendant of it.
fn tree(root: Pid) -> Vec<Pid> {
    let parents = parents();
    let mut tree = Vec::new();
    if parents.contains_key(&root) {
        tree.push(root);
    }
    let mut index = 0;
    let mut frontier = vec![root];
    while index < frontier.len() {
        let parent = frontier[index];
        index += 1;
        for (&pid, &its_parent) in &parents {
            if its_parent == parent && pid != parent && !frontier.contains(&pid) {
                frontier.push(pid);
                tree.push(pid);
            }
        }
    }
    tree
}

/// Whether the process or any of its descendants is alive.
pub(super) fn process_group_exists(group: Pid) -> bool {
    !tree(group).is_empty()
}

/// SIGINT does nothing (Ctrl-Break can't reach another console); any other
/// signal terminates the process and its descendants.
pub(super) fn signal_process_group(group: Pid, signal: i32) {
    if signal != SIGINT {
        tree(group).into_iter().for_each(terminate);
    }
}

/// SIGINT does nothing; any other signal terminates the process.
pub(super) fn signal_process(pid: Pid, signal: i32) {
    if signal != SIGINT {
        terminate(pid);
    }
}

fn terminate(pid: Pid) {
    if let Some(process) = open(pid, PROCESS_TERMINATE) {
        unsafe { TerminateProcess(process.as_raw_handle(), TERMINATED_EXIT_CODE) };
    }
}

/// Windows has no process groups to share: localhost's "group" is itself.
pub(super) fn own_process_group() -> Pid {
    unsafe { GetCurrentProcessId() as Pid }
}

pub(super) fn leads_session(_group: Pid) -> bool {
    false
}
