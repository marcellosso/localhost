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
