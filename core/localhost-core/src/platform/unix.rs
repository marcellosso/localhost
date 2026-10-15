//! Process groups and signals, the same on every Unix.

use super::Pid;

pub(super) fn process_group_exists(group: Pid) -> bool {
    unsafe { libc::kill(-group, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

pub(super) fn signal_process_group(group: Pid, signal: i32) {
    unsafe {
        libc::kill(-group, signal);
    }
}

pub(super) fn signal_process(pid: Pid, signal: i32) {
    unsafe {
        libc::kill(pid, signal);
    }
}

pub(super) fn own_process_group() -> Pid {
    unsafe { libc::getpgrp() }
}

pub(super) fn leads_session(group: Pid) -> bool {
    unsafe { libc::getsid(group) == group }
}
