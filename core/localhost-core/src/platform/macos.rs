use std::collections::HashMap;
use std::ffi::CStr;
use std::mem::{MaybeUninit, size_of};
use std::sync::OnceLock;
use std::time::Duration;

use super::{ListeningProcess, ProcessRelations, ProcessStartTime, ProcessUsage, normalized};

unsafe extern "C" {
    fn localhost_listening_ports(pid: libc::pid_t, ports: *mut u16, capacity: libc::c_int) -> libc::c_int;
    // Declared here: the libc crate deprecates its own declaration.
    fn mach_timebase_info(info: *mut MachTimebase) -> libc::c_int;
}

/// `mach_timebase_info_data_t`.
#[repr(C)]
struct MachTimebase {
    numerator: u32,
    denominator: u32,
}

/// `<sys/proc_info.h>`'s `PROC_ALL_PIDS`, which the libc crate lacks.
const PROC_ALL_PIDS: u32 = 1;
/// `<sys/proc.h>`'s `SZOMB`: the process has ended and waits to be reaped.
const SZOMB: u32 = 5;

fn bsd_info(pid: libc::pid_t) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let filled = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, info.as_mut_ptr().cast(), size) };
    (filled == size).then(|| unsafe { info.assume_init() })
}

pub(super) fn start_time(pid: libc::pid_t) -> Option<ProcessStartTime> {
    let info = bsd_info(pid)?;
    Some(ProcessStartTime { seconds: info.pbi_start_tvsec, microseconds: info.pbi_start_tvusec })
}

fn all_pids() -> Vec<libc::pid_t> {
    let capacity = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if capacity <= 0 {
        return Vec::new();
    }
    // Room for processes started between the two calls.
    let mut pids = vec![0 as libc::pid_t; capacity as usize / size_of::<libc::pid_t>() + 64];
    let bytes = (pids.len() * size_of::<libc::pid_t>()) as libc::c_int;
    let size = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(size.max(0) as usize / size_of::<libc::pid_t>());
    pids.retain(|&pid| pid > 0);
    pids
}

fn c_chars(chars: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn listening_ports(pid: libc::pid_t) -> Vec<u16> {
    let mut ports = [0u16; 256];
    let count = unsafe { localhost_listening_ports(pid, ports.as_mut_ptr(), ports.len() as libc::c_int) };
    normalized(ports[..count.max(0) as usize].to_vec())
}

fn executable_path(pid: libc::pid_t) -> Option<String> {
    // PROC_PIDPATHINFO_MAXSIZE.
    let mut buffer = vec![0u8; 4 * libc::PATH_MAX as usize];
    let length = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    (length > 0).then(|| String::from_utf8_lossy(&buffer[..length as usize]).into_owned())
}

fn working_directory(pid: libc::pid_t) -> Option<String> {
    let mut info = MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let size = size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let filled = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDVNODEPATHINFO, 0, info.as_mut_ptr().cast(), size) };
    if filled != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some(c_chars(info.pvi_cdir.vip_path.as_flattened())).filter(|path| !path.is_empty())
}

/// The process's arguments joined by spaces, from `KERN_PROCARGS2`: argc,
/// the executable path, padding, then the arguments.
fn command_line(pid: libc::pid_t) -> Option<String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 0;
    let ok = unsafe { libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) };
    if ok != 0 || size <= size_of::<i32>() {
        return None;
    }
    let mut buffer = vec![0u8; size];
    let ok =
        unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buffer.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) };
    if ok != 0 {
        return None;
    }
    buffer.truncate(size);
    let argc = i32::from_ne_bytes(buffer[..4].try_into().ok()?);
    let mut index = 4;
    // Skip the executable path and the NULs after it.
    while index < buffer.len() && buffer[index] != 0 {
        index += 1;
    }
    while index < buffer.len() && buffer[index] == 0 {
        index += 1;
    }
    let mut arguments = Vec::new();
    while (arguments.len() as i32) < argc && index < buffer.len() {
        let end = buffer[index..].iter().position(|&b| b == 0).map_or(buffer.len(), |offset| index + offset);
        arguments.push(String::from_utf8_lossy(&buffer[index..end]).into_owned());
        index = end + 1;
    }
    (!arguments.is_empty()).then(|| arguments.join(" "))
}

pub(super) fn scan_listeners() -> Vec<ListeningProcess> {
    all_pids()
        .into_iter()
        .filter_map(|pid| {
            let ports = listening_ports(pid);
            if ports.is_empty() {
                return None;
            }
            let info = bsd_info(pid)?;
            let name = unsafe { CStr::from_ptr(info.pbi_name.as_ptr()) }.to_string_lossy().into_owned();
            let short_name = unsafe { CStr::from_ptr(info.pbi_comm.as_ptr()) }.to_string_lossy().into_owned();
            Some(ListeningProcess {
                pid,
                process_group_id: info.pbi_pgid as libc::pid_t,
                session_id: unsafe { libc::getsid(pid) },
                command: command_line(pid).unwrap_or_else(|| short_name.clone()),
                name: if name.is_empty() { short_name } else { name },
                executable_path: executable_path(pid),
                ports,
                working_directory: working_directory(pid),
                start_time: ProcessStartTime { seconds: info.pbi_start_tvsec, microseconds: info.pbi_start_tvusec },
            })
        })
        .collect()
}

pub(super) fn parents() -> HashMap<libc::pid_t, libc::pid_t> {
    all_pids().into_iter().filter_map(|pid| Some((pid, bsd_info(pid)?.pbi_ppid as libc::pid_t))).collect()
}

pub(super) fn relations() -> Vec<ProcessRelations> {
    all_pids()
        .into_iter()
        .filter_map(|pid| {
            let info = bsd_info(pid).filter(|info| info.pbi_status != SZOMB)?;
            Some(ProcessRelations {
                pid,
                parent: info.pbi_ppid as libc::pid_t,
                process_group: info.pbi_pgid as libc::pid_t,
                session: unsafe { libc::getsid(pid) },
            })
        })
        .collect()
}

pub(super) fn usage(pid: libc::pid_t) -> Option<ProcessUsage> {
    if bsd_info(pid)?.pbi_status == SZOMB {
        return None;
    }
    let mut info = MaybeUninit::<libc::rusage_info_v2>::zeroed();
    if unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast()) } != 0 {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let ticks = info.ri_user_time + info.ri_system_time + info.ri_child_user_time + info.ri_child_system_time;
    Some(ProcessUsage { cpu_time: mach_ticks(ticks), memory_bytes: info.ri_phys_footprint })
}

/// `rusage_info`'s times are in Mach absolute time units, which are
/// nanoseconds on Intel but not on Apple silicon.
fn mach_ticks(ticks: u64) -> Duration {
    static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
    let &(numerator, denominator) = TIMEBASE.get_or_init(|| {
        let mut timebase = MachTimebase { numerator: 0, denominator: 0 };
        match unsafe { mach_timebase_info(&mut timebase) } {
            0 if timebase.denominator != 0 => (timebase.numerator as u64, timebase.denominator as u64),
            _ => (1, 1),
        }
    });
    Duration::from_nanos((ticks as u128 * numerator as u128 / denominator as u128) as u64)
}

pub(super) fn managed_jobs() -> HashMap<libc::pid_t, String> {
    let Ok(output) = std::process::Command::new("/bin/launchctl").arg("list").output() else {
        return HashMap::new();
    };
    parse_launchctl_list(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `launchctl list`: `PID\tStatus\tLabel` lines, PID `-` when not running.
pub(crate) fn parse_launchctl_list(output: &str) -> HashMap<libc::pid_t, String> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let pid = fields.next()?.parse().ok()?;
            let _status = fields.next()?;
            Some((pid, fields.next()?.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn reads_the_running_launchd_jobs_from_launchctl_list() {
        let output = "PID\tStatus\tLabel\n412\t0\thomebrew.mxcl.postgresql@16\n-\t0\tcom.apple.idle\n77\t-9\tapplication.com.example.1.2\n";
        assert_eq!(
            super::parse_launchctl_list(output),
            HashMap::from([(412, "homebrew.mxcl.postgresql@16".into()), (77, "application.com.example.1.2".into())])
        );
    }
}
