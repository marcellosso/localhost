//! Processes on Windows. Runs live in Job Objects (`process::windows`); what
//! is here serves everything else that names a process by its pid: Orphans
//! and Listeners. Windows has no process-group signals and Ctrl-Break only
//! reaches processes on localhost's own console, so a "group" here is a
//! process and its descendants, and only terminating them stops them
//! (`docs/cross-platform.md`, Native Windows).
//!
//! Listeners come from `GetExtendedTcpTable`. A process's command line and
//! working directory are read from its PEB, as psutil does; that needs the
//! right to read its memory, so an elevated or protected process has neither.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, FILETIME, INVALID_HANDLE_VALUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW, SC_ENUM_PROCESS_INFO,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetProcessTimes, IsWow64Process, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ, QueryFullProcessImageNameW,
    TerminateProcess,
};

use super::signal::SIGINT;
use super::{ListeningProcess, Pid, ProcessStartTime, normalized};

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

/// A process in a Toolhelp snapshot.
struct ProcessEntry {
    pid: Pid,
    parent: Pid,
    /// The executable's file name, e.g. `node.exe`.
    name: String,
}

/// Every live process, from a Toolhelp snapshot.
fn processes() -> Vec<ProcessEntry> {
    let mut processes = Vec::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return processes;
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) } != 0;
    while found {
        processes.push(ProcessEntry {
            pid: entry.th32ProcessID as Pid,
            parent: entry.th32ParentProcessID as Pid,
            name: from_wide(&entry.szExeFile),
        });
        found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    processes
}

/// Text from UTF-16, up to the first NUL.
fn from_wide(text: &[u16]) -> String {
    let end = text.iter().position(|&unit| unit == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

pub(super) fn scan_listeners() -> Vec<ListeningProcess> {
    let mut ports: HashMap<Pid, Vec<u16>> = HashMap::new();
    for (pid, port) in listening_sockets() {
        ports.entry(pid).or_default().push(port);
    }
    if ports.is_empty() {
        return Vec::new();
    }
    let names: HashMap<Pid, String> = processes().into_iter().map(|process| (process.pid, process.name)).collect();
    ports
        .into_iter()
        .filter_map(|(pid, ports)| {
            let start_time = start_time(pid)?;
            let executable_path = executable_path(pid);
            let name = names
                .get(&pid)
                .cloned()
                .or_else(|| executable_path.as_deref().and_then(|path| path.rsplit('\\').next()).map(String::from))
                .unwrap_or_else(|| format!("PID {pid}"));
            let parameters = process_parameters(pid);
            let (command, working_directory) = match parameters {
                Some(parameters) => (Some(parameters.command_line), Some(parameters.current_directory)),
                None => (None, None),
            };
            Some(ListeningProcess {
                pid,
                process_group_id: pid,
                command: command.filter(|command| !command.is_empty()).unwrap_or_else(|| name.clone()),
                name,
                executable_path,
                ports: normalized(ports),
                // Long names and links resolved, as Project folders are: a
                // process started in `C:\Users\RUNNER~1` is in `C:\Users\runneradmin`.
                working_directory: working_directory
                    .map(|directory| super::resolved_path(&without_trailing_separator(&directory))),
                start_time,
            })
        })
        .collect()
}

/// The owning pid and local port of every listening TCP socket, IPv4 and IPv6.
fn listening_sockets() -> Vec<(Pid, u16)> {
    let v4 = tcp_table::<MIB_TCPROW_OWNER_PID>(AF_INET).into_iter().map(|row| (row.dwOwningPid, row.dwLocalPort));
    let v6 = tcp_table::<MIB_TCP6ROW_OWNER_PID>(AF_INET6).into_iter().map(|row| (row.dwOwningPid, row.dwLocalPort));
    v4.chain(v6)
        .filter_map(|(pid, port)| {
            // The port is in network byte order, in the low 16 bits.
            let port = u16::from_be(port as u16);
            (pid != 0 && port != 0).then_some((pid as Pid, port))
        })
        .collect()
}

/// The rows of `GetExtendedTcpTable`'s listening sockets for one address
/// family: a row count followed by the rows.
fn tcp_table<Row: Copy>(family: u16) -> Vec<Row> {
    let mut size = 0u32;
    let mut buffer: Vec<u32> = Vec::new();
    // The table may grow between asking for its size and reading it.
    for _ in 0..4 {
        let result = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut size,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        match result {
            0 => return rows_after_count(&buffer),
            ERROR_INSUFFICIENT_BUFFER => buffer = vec![0; (size as usize).div_ceil(4)],
            _ => return Vec::new(),
        }
    }
    Vec::new()
}

/// Rows following a `u32` count, as Windows lays out its tables.
fn rows_after_count<Row: Copy>(buffer: &[u32]) -> Vec<Row> {
    let Some(&count) = buffer.first() else { return Vec::new() };
    let available = (buffer.len() - 1) * 4 / size_of::<Row>();
    let rows = unsafe { buffer.as_ptr().add(1).cast::<Row>() };
    (0..(count as usize).min(available)).map(|index| unsafe { rows.add(index).read_unaligned() }).collect()
}

fn executable_path(pid: Pid) -> Option<String> {
    let process = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut path = vec![0u16; 32_768];
    let mut length = path.len() as u32;
    let succeeded = unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut length)
    };
    (succeeded != 0).then(|| String::from_utf16_lossy(&path[..length as usize]))
}

fn session_of(pid: Pid) -> Option<u32> {
    let mut session = 0;
    (unsafe { ProcessIdToSessionId(pid as u32, &mut session) } != 0).then_some(session)
}

/// `C:\app\` → `C:\app`, as working directories are compared; a drive's root keeps its separator.
fn without_trailing_separator(path: &str) -> String {
    let trimmed = path.trim_end_matches('\\');
    if trimmed.ends_with(':') { format!("{trimmed}\\") } else { trimmed.to_string() }
}

/// What a process's PEB holds about how it was started.
struct ProcessParameters {
    command_line: String,
    current_directory: String,
}

/// `PROCESS_BASIC_INFORMATION`, with the PEB's address as a number.
#[repr(C)]
struct BasicInformation {
    exit_status: i32,
    peb: usize,
    affinity_mask: usize,
    base_priority: i32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
}

/// A `UNICODE_STRING` in another process: its length in bytes and where its text is.
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: usize,
}

/// Offsets into the PEB and `RTL_USER_PROCESS_PARAMETERS`, for a process of
/// localhost's own bitness (the layout is undocumented but stable since NT).
#[cfg(target_pointer_width = "64")]
mod offsets {
    pub const PROCESS_PARAMETERS: usize = 0x20;
    pub const CURRENT_DIRECTORY: usize = 0x38;
    pub const COMMAND_LINE: usize = 0x70;
}
#[cfg(target_pointer_width = "32")]
mod offsets {
    pub const PROCESS_PARAMETERS: usize = 0x10;
    pub const CURRENT_DIRECTORY: usize = 0x24;
    pub const COMMAND_LINE: usize = 0x40;
}

/// The command line and working directory from the process's PEB. `None`
/// if its memory can't be read, or it is a 32-bit process on 64-bit Windows
/// (whose PEB has the other layout).
fn process_parameters(pid: Pid) -> Option<ProcessParameters> {
    let process = open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
    let mut is_wow64 = 0;
    if unsafe { IsWow64Process(process.as_raw_handle(), &mut is_wow64) } == 0
        || (is_wow64 != 0) != cfg!(target_pointer_width = "32")
    {
        return None;
    }
    let mut information = MaybeUninit::<BasicInformation>::zeroed();
    let status = unsafe {
        NtQueryInformationProcess(
            process.as_raw_handle(),
            ProcessBasicInformation,
            information.as_mut_ptr().cast(),
            size_of::<BasicInformation>() as u32,
            ptr::null_mut(),
        )
    };
    if status < 0 {
        return None;
    }
    let peb = unsafe { information.assume_init() }.peb;
    let parameters: usize = read(&process, peb + offsets::PROCESS_PARAMETERS)?;
    let command_line: UnicodeString = read(&process, parameters + offsets::COMMAND_LINE)?;
    let current_directory: UnicodeString = read(&process, parameters + offsets::CURRENT_DIRECTORY)?;
    Some(ProcessParameters {
        command_line: read_text(&process, command_line)?,
        current_directory: read_text(&process, current_directory)?,
    })
}

/// A value from another process's memory.
fn read<T: Copy>(process: &OwnedHandle, address: usize) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut count = 0;
    let succeeded = unsafe {
        ReadProcessMemory(
            process.as_raw_handle(),
            address as *const c_void,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            &mut count,
        )
    };
    (succeeded != 0 && count == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

fn read_text(process: &OwnedHandle, string: UnicodeString) -> Option<String> {
    let mut text = vec![0u16; string.length as usize / 2];
    if text.is_empty() {
        return Some(String::new());
    }
    let mut count = 0;
    let succeeded = unsafe {
        ReadProcessMemory(
            process.as_raw_handle(),
            string.buffer as *const c_void,
            text.as_mut_ptr().cast(),
            text.len() * 2,
            &mut count,
        )
    };
    (succeeded != 0 && count == text.len() * 2).then(|| String::from_utf16_lossy(&text))
}

/// The parent of every live process. A parent may have ended (Windows
/// doesn't reparent), so its pid may be reused: callers pair this with start
/// times where that matters.
pub(super) fn parents() -> HashMap<Pid, Pid> {
    processes().into_iter().map(|process| (process.pid, process.parent)).collect()
}

/// What Windows runs and would start again, which localhost never stops:
/// every Windows service's process, labelled with its services' names, and
/// (unless localhost itself runs there, e.g. over SSH) every other process
/// of session 0, where services live, labelled with its executable's name.
pub(super) fn managed_jobs() -> HashMap<Pid, String> {
    let mut jobs = services();
    let own_session = session_of(unsafe { GetCurrentProcessId() } as Pid);
    if own_session != Some(0) {
        for process in processes() {
            if process.pid != 0 && session_of(process.pid) == Some(0) {
                jobs.entry(process.pid).or_insert(process.name);
            }
        }
    }
    jobs
}

/// The running Windows services, by process: their names, comma-separated
/// (one `svchost.exe` may host several).
fn services() -> HashMap<Pid, String> {
    let mut names: HashMap<Pid, Vec<String>> = HashMap::new();
    let manager = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_ENUMERATE_SERVICE) };
    if manager.is_null() {
        return HashMap::new();
    }
    // 64 KiB holds a few hundred services; more come in further calls.
    let mut buffer = vec![0u64; 8 * 1024];
    let mut resume = 0u32;
    loop {
        let (mut needed, mut count) = (0u32, 0u32);
        let succeeded = unsafe {
            EnumServicesStatusExW(
                manager,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_ACTIVE,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
                &mut count,
                &mut resume,
                ptr::null(),
            )
        } != 0;
        let more = !succeeded && unsafe { windows_sys::Win32::Foundation::GetLastError() } == ERROR_MORE_DATA;
        let services = buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
        for index in 0..count as usize {
            let service = unsafe { &*services.add(index) };
            let pid = service.ServiceStatusProcess.dwProcessId;
            if pid != 0 {
                names.entry(pid as Pid).or_default().push(unsafe { wide_at(service.lpServiceName) });
            }
        }
        if !more {
            break;
        }
        if count == 0 {
            buffer = vec![0u64; (needed as usize).div_ceil(8)];
        }
    }
    unsafe { CloseServiceHandle(manager) };
    names
        .into_iter()
        .map(|(pid, mut names)| {
            names.sort();
            (pid, names.join(", "))
        })
        .collect()
}

/// A NUL-terminated UTF-16 string.
unsafe fn wide_at(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
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
