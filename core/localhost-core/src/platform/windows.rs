//! Windows sees none of the processes localhost runs: they live in a WSL
//! distro, where `localhostd` inspects them with the Linux code (ADR-0008).
//! Native Windows execution (XP5) fills this in.

use std::collections::HashMap;

use super::{ListeningProcess, Pid, ProcessStartTime};

pub(super) fn start_time(_pid: Pid) -> Option<ProcessStartTime> {
    None
}

pub(super) fn scan_listeners() -> Vec<ListeningProcess> {
    Vec::new()
}

pub(super) fn parents() -> HashMap<Pid, Pid> {
    HashMap::new()
}

pub(super) fn managed_jobs() -> HashMap<Pid, String> {
    HashMap::new()
}

pub(super) fn process_group_exists(_group: Pid) -> bool {
    false
}

pub(super) fn signal_process_group(_group: Pid, _signal: i32) {}

pub(super) fn signal_process(_pid: Pid, _signal: i32) {}

pub(super) fn own_process_group() -> Pid {
    0
}

pub(super) fn leads_session(_group: Pid) -> bool {
    false
}
