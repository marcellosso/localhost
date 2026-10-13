//! Process information from `/proc`. Listening sockets come from
//! `/proc/net/tcp{,6}` (state `0A`, LISTEN) and are matched to processes
//! through the `socket:[inode]` links in `/proc/<pid>/fd`; only this user's
//! processes can be read, as on macOS.

use std::collections::{HashMap, HashSet};
use std::fs;

use super::{ListeningProcess, ProcessStartTime, normalized};

/// The fields of `/proc/<pid>/stat` localhost uses.
struct Stat {
    parent: libc::pid_t,
    process_group: libc::pid_t,
    session: libc::pid_t,
    /// Clock ticks since boot.
    start_ticks: u64,
}

/// Parses `/proc/<pid>/stat`. The command name is in parentheses and may
/// itself contain spaces and parentheses, so fields are read after the last `)`.
fn parse_stat(text: &str) -> Option<Stat> {
    let after_name = &text[text.rfind(')')? + 1..];
    // Field 3 (state) is the first one after the name.
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    let field = |number: usize| fields.get(number - 3).copied();
    Some(Stat {
        parent: field(4)?.parse().ok()?,
        process_group: field(5)?.parse().ok()?,
        session: field(6)?.parse().ok()?,
        start_ticks: field(22)?.parse().ok()?,
    })
}

fn stat(pid: libc::pid_t) -> Option<Stat> {
    parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

fn ticks_per_second() -> u64 {
    match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
        ticks if ticks > 0 => ticks as u64,
        _ => 100,
    }
}

pub(super) fn start_time(pid: libc::pid_t) -> Option<ProcessStartTime> {
    let ticks = stat(pid)?.start_ticks;
    let hz = ticks_per_second();
    Some(ProcessStartTime { seconds: ticks / hz, microseconds: ticks % hz * 1_000_000 / hz })
}

fn all_pids() -> Vec<libc::pid_t> {
    let Ok(entries) = fs::read_dir("/proc") else { return Vec::new() };
    entries.filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok()).collect()
}

/// The inodes of listening TCP sockets, with their ports.
fn listening_sockets() -> HashMap<u64, u16> {
    let mut sockets = HashMap::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(text) = fs::read_to_string(table) {
            sockets.extend(parse_tcp_table(&text));
        }
    }
    sockets
}

/// Parses `/proc/net/tcp` or `tcp6`: the inode and local port of every socket in LISTEN.
fn parse_tcp_table(text: &str) -> Vec<(u64, u16)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (local, state, inode) = (fields.get(1)?, fields.get(3)?, fields.get(9)?);
            if *state != "0A" {
                return None;
            }
            let port = u16::from_str_radix(local.rsplit_once(':')?.1, 16).ok()?;
            let inode = inode.parse().ok()?;
            (port != 0 && inode != 0).then_some((inode, port))
        })
        .collect()
}

fn socket_inodes(pid: libc::pid_t) -> Vec<u64> {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else { return Vec::new() };
    entries
        .filter_map(|entry| {
            let target = fs::read_link(entry.ok()?.path()).ok()?;
            target.to_str()?.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
        })
        .collect()
}

fn read_link(pid: libc::pid_t, name: &str) -> Option<String> {
    let path = fs::read_link(format!("/proc/{pid}/{name}")).ok()?;
    Some(path.to_string_lossy().into_owned())
}

fn command_line(pid: libc::pid_t) -> Option<String> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let arguments: Vec<String> =
        bytes.split(|&b| b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    (!arguments.is_empty()).then(|| arguments.join(" "))
}

/// The process's name: `comm`, which the kernel cuts at 15 bytes, or the
/// executable's file name when that is what was cut.
fn name(pid: libc::pid_t, executable_path: Option<&str>) -> String {
    let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim_end().to_string();
    let file_name = executable_path.and_then(|path| path.rsplit('/').next()).unwrap_or_default();
    if comm.len() >= 15 && file_name.starts_with(&comm) { file_name.to_string() } else { comm }
}

pub(super) fn scan_listeners() -> Vec<ListeningProcess> {
    let sockets = listening_sockets();
    if sockets.is_empty() {
        return Vec::new();
    }
    all_pids()
        .into_iter()
        .filter_map(|pid| {
            let ports: HashSet<u16> =
                socket_inodes(pid).into_iter().filter_map(|inode| sockets.get(&inode).copied()).collect();
            if ports.is_empty() {
                return None;
            }
            let stat = stat(pid)?;
            let executable_path = read_link(pid, "exe").map(|path| path.trim_end_matches(" (deleted)").to_string());
            let name = name(pid, executable_path.as_deref());
            let hz = ticks_per_second();
            Some(ListeningProcess {
                pid,
                process_group_id: stat.process_group,
                session_id: stat.session,
                command: command_line(pid).unwrap_or_else(|| name.clone()),
                name,
                executable_path,
                ports: normalized(ports.into_iter().collect()),
                working_directory: read_link(pid, "cwd"),
                start_time: ProcessStartTime {
                    seconds: stat.start_ticks / hz,
                    microseconds: stat.start_ticks % hz * 1_000_000 / hz,
                },
            })
        })
        .collect()
}

pub(super) fn parents() -> HashMap<libc::pid_t, libc::pid_t> {
    all_pids().into_iter().filter_map(|pid| Some((pid, stat(pid)?.parent))).collect()
}

/// Linux has no launchd; systemd units are left to future work.
pub(super) fn managed_jobs() -> HashMap<libc::pid_t, String> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat_even_when_the_name_has_spaces_and_parentheses() {
        let text = "4242 (my (weird) name) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 0 0";
        let stat = parse_stat(text).unwrap();
        assert_eq!((stat.parent, stat.process_group, stat.session, stat.start_ticks), (1, 4242, 4242, 987654));
    }

    #[test]
    fn parses_listening_sockets_from_the_tcp_tables() {
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
           0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 31337 1 0 100 0 0 10 0\n\
           1: 0100007F:B3A2 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 31338 1 0 20 4 30 10 -1\n";
        assert_eq!(parse_tcp_table(text), [(31337, 8080)]);
        let v6 = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
           0: 00000000000000000000000001000000:1435 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0 100 0 0 10 0\n";
        assert_eq!(parse_tcp_table(v6), [(4242, 5173)]);
    }
}
