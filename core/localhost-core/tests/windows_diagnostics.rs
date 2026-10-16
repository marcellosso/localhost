//! Temporary (XP5): prints what Windows does, without localhost in between,
//! so CI shows why a process-tree test fails. Removed once those pass.
#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent, GetConsoleProcessList};
use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

#[test]
fn print_what_windows_does() {
    let mut process = 0;
    eprintln!("diagnostic: console processes = {}", unsafe { GetConsoleProcessList(&mut process, 1) });

    let status = Command::new("cmd.exe").raw_arg("/s /c \"definitely-not-a-command-localhost\"").output().unwrap();
    eprintln!(
        "diagnostic: unknown command exit = {:?}, stderr = {:?}",
        status.status.code(),
        String::from_utf8_lossy(&status.stderr)
    );

    for (label, program, arguments) in [
        ("python", "python", "-c \"import time; time.sleep(30)\""),
        ("cmd running python", "cmd.exe", "/s /c \"python -c \"import time; time.sleep(30)\"\""),
    ] {
        let mut child = Command::new(program)
            .raw_arg(arguments)
            .stdin(Stdio::null())
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
        let error = unsafe { GetLastError() };
        let start = Instant::now();
        let mut exit = None;
        while start.elapsed() < Duration::from_secs(10) {
            if let Some(status) = child.try_wait().unwrap() {
                exit = Some(status.code());
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("diagnostic: {label}: sent = {sent}, error = {error}, exit after Ctrl-Break = {exit:?}");
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Ctrl-Break with one of localhost's launch choices at a time.
#[test]
fn print_which_launch_choice_stops_ctrl_break() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    fn resume(pid: u32) {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let mut found = Thread32First(snapshot, &mut entry) != 0;
            while found {
                if entry.th32OwnerProcessID == pid {
                    ResumeThread(OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID));
                    return;
                }
                found = Thread32Next(snapshot, &mut entry) != 0;
            }
        }
    }

    for (label, piped, suspended, job) in [
        ("piped", true, false, false),
        ("suspended", false, true, false),
        ("job", false, false, true),
        ("all three", true, true, true),
    ] {
        let mut command = Command::new("cmd.exe");
        command.raw_arg("/s /c \"python -c \"import time; time.sleep(30)\"\"").stdin(Stdio::null());
        if piped {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        let flags = CREATE_NEW_PROCESS_GROUP | if suspended { CREATE_SUSPENDED } else { 0 };
        let mut child = command.creation_flags(flags).spawn().unwrap();
        if job {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                eprintln!(
                    "diagnostic: {label}: assigned = {}",
                    AssignProcessToJobObject(handle, child.as_raw_handle())
                );
                // Left open: closing it would kill the job.
            }
        }
        if suspended {
            resume(child.id());
        }
        std::thread::sleep(Duration::from_secs(2));
        let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
        let start = Instant::now();
        let mut exit = None;
        while start.elapsed() < Duration::from_secs(10) {
            if let Some(status) = child.try_wait().unwrap() {
                exit = Some(status.code());
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("diagnostic: {label}: sent = {sent}, exit after Ctrl-Break = {exit:?}");
        let _ = child.kill();
        let _ = child.wait();
    }
}
