//! Launching and stopping a Run on Windows: `cmd.exe /s /c "<command line>"`
//! in a new process group, inside its own kill-on-close Job Object
//! (ADR-0010, ADR-0011). The job is the Run's group: membership is exact,
//! nothing can break away, and when localhost's process ends (crash and
//! force-kill included) Windows kills every Run's tree.
//!
//! Only a process sharing a console can send it Ctrl-Break. A GUI app has
//! none, so localhost attaches once to a console without a window, and every
//! Run inherits it. The CLI keeps its terminal's console.

use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;
use std::{mem, ptr};

use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::oneshot;
use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, GenerateConsoleCtrlEvent, GetConsoleProcessList,
    SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenProcess, OpenThread,
    PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};

use super::LaunchConfiguration;
use crate::platform::signal::{SIGINT, SIGKILL};
use crate::platform::{ListeningProcess, Pid};
use crate::process::Termination;
use crate::{LocalhostError, RunEnvironment};

/// Ctrl-Break, then terminating the job: Windows has nothing like SIGTERM.
pub(super) const STOP_SIGNALS: [i32; 2] = [SIGINT, SIGKILL];
/// What the shell left behind is terminated: Ctrl-Break is only sent while
/// the shell, whose pid names the process group, is still running.
pub(super) const LEFTOVER_SIGNAL: i32 = SIGKILL;
/// How long the job may take to empty after the shell exits before what is
/// in it counts as left over: Windows counts an exited process out of its
/// job slightly after the process is signalled.
pub(super) const LEFTOVER_GRACE: Duration = Duration::from_millis(200);
/// The exit code of processes ended by terminating the job.
const TERMINATED_EXIT_CODE: u32 = 1;

/// The Run's Job Object, and the shell, whose pid is the process group id.
pub(super) struct Group {
    pid: Pid,
    job: OwnedHandle,
    /// Held so the shell's pid (and with it the group id) isn't reused.
    shell: OwnedHandle,
}

impl Group {
    /// Whether any process is left in the job.
    pub(super) fn exists(&self) -> bool {
        let mut information: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { mem::zeroed() };
        let succeeded = unsafe {
            QueryInformationJobObject(
                raw(&self.job),
                JobObjectBasicAccountingInformation,
                (&raw mut information).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                ptr::null_mut(),
            )
        };
        succeeded == 0 || information.ActiveProcesses > 0
    }

    /// Whether the process is in the job. A process ends up in it through
    /// the shell only, so this is exact.
    pub(super) fn contains(&self, process: &ListeningProcess) -> bool {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process.pid as u32) };
        if handle.is_null() {
            return false;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut is_in_job = 0;
        unsafe { IsProcessInJob(raw(&handle), raw(&self.job), &mut is_in_job) != 0 && is_in_job != 0 }
    }

    /// SIGINT sends Ctrl-Break to the process group, anything else terminates the job.
    pub(super) fn send(&self, signal: i32) {
        unsafe {
            if signal != SIGINT {
                TerminateJobObject(raw(&self.job), TERMINATED_EXIT_CODE);
            } else if WaitForSingleObject(raw(&self.shell), 0) == WAIT_TIMEOUT {
                // A group id whose leader is gone would reach every process on the console.
                GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, self.pid as u32);
            }
        }
    }
}

pub(super) struct Launched {
    pub(super) pid: Pid,
    pub(super) group: Group,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
    pub(super) shell_termination: oneshot::Receiver<Termination>,
}

pub(super) fn launch(configuration: &LaunchConfiguration) -> Result<Launched, LocalhostError> {
    attach_to_a_console().map_err(launch_failed)?;
    let job = kill_on_close_job().map_err(launch_failed)?;

    let mut command = Command::new(&configuration.shell);
    // Passed verbatim: `/s` makes cmd take everything between the outer quotes as is.
    command
        .raw_arg(format!("/s /c \"{}\"", configuration.command_line))
        .current_dir(&configuration.working_directory)
        .env_clear()
        .envs(configuration.environment.iter().filter(|(name, _)| !name.is_empty() && !name.contains('=')))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // No window flags: the shell inherits localhost's console.
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED);
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => LocalhostError::ShellNotFound(configuration.shell.clone()),
        _ => launch_failed(error.raw_os_error().unwrap_or(0) as u32),
    })?;

    // Until it is resumed in the job, the shell has run nothing: kill it on any failure.
    let prepared = (|| {
        let stdout = ChildStdout::from_std(child.stdout.take().expect("piped")).map_err(io_failed)?;
        let stderr = ChildStderr::from_std(child.stderr.take().expect("piped")).map_err(io_failed)?;
        let shell = child.as_handle().try_clone_to_owned().map_err(io_failed)?;
        if unsafe { AssignProcessToJobObject(raw(&job), child.as_raw_handle()) } == 0 {
            return Err(launch_failed(last_error()));
        }
        let thread = only_thread(child.id()).map_err(launch_failed)?;
        Ok((stdout, stderr, shell, thread))
    })();
    let (stdout, stderr, shell, thread) = match prepared {
        Ok(prepared) => prepared,
        Err(failure) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failure);
        }
    };
    unsafe { ResumeThread(raw(&thread)) };

    let pid = child.id() as Pid;
    let (sender, shell_termination) = oneshot::channel();
    std::thread::Builder::new()
        .name(format!("localhost-wait-{pid}"))
        .spawn(move || {
            let code = child.wait().ok().and_then(|status| status.code()).unwrap_or(TERMINATED_EXIT_CODE as i32);
            let _ = sender.send(Termination::Exited { code });
        })
        .expect("a thread to wait for the shell");
    Ok(Launched { pid, group: Group { pid, job, shell }, stdout, stderr, shell_termination })
}

fn kill_on_close_job() -> Result<OwnedHandle, u32> {
    let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if handle.is_null() {
        return Err(last_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let succeeded = unsafe {
        SetInformationJobObject(
            raw(&job),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if succeeded == 0 { Err(last_error()) } else { Ok(job) }
}

/// The one thread of a process created suspended, to resume it.
/// (`ChildExt::main_thread_handle` would do, once stable.)
fn only_thread(pid: u32) -> Result<OwnedHandle, u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry: THREADENTRY32 = unsafe { mem::zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    let mut found = unsafe { Thread32First(raw(&snapshot), &mut entry) } != 0;
    while found {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            return if thread.is_null() {
                Err(last_error())
            } else {
                Ok(unsafe { OwnedHandle::from_raw_handle(thread) })
            };
        }
        found = unsafe { Thread32Next(raw(&snapshot), &mut entry) } != 0;
    }
    Err(last_error())
}

/// Makes sure this process has a console its Runs inherit. Without one (a
/// GUI app), starts `cmd.exe` with a console but no window, attaches to that
/// console and ends the `cmd.exe`: the console lasts while localhost is
/// attached. Done once; the outcome is kept.
fn attach_to_a_console() -> Result<(), u32> {
    static ATTACHED: OnceLock<Result<(), u32>> = OnceLock::new();
    *ATTACHED.get_or_init(|| {
        let mut process = 0;
        if unsafe { GetConsoleProcessList(&mut process, 1) } > 0 {
            return Ok(());
        }
        let shell = RunEnvironment::shell(&RunEnvironment::current());
        let mut host = Command::new(shell)
            .raw_arg("/d /q /k")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| error.raw_os_error().unwrap_or(0) as u32)?;
        let mut attached = Err(0);
        for _ in 0..200 {
            if unsafe { AttachConsole(host.id()) } != 0 {
                attached = Ok(());
                break;
            }
            attached = Err(last_error());
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = host.kill();
        let _ = host.wait();
        attached?;
        // Ctrl-C and Ctrl-Break are for the Runs: never let them end localhost.
        unsafe { SetConsoleCtrlHandler(Some(ignore_interrupts), 1) };
        Ok(())
    })
}

unsafe extern "system" fn ignore_interrupts(event: u32) -> windows_sys::core::BOOL {
    (event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT) as windows_sys::core::BOOL
}

fn raw(handle: &OwnedHandle) -> RawHandle {
    handle.as_raw_handle()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

fn launch_failed(code: u32) -> LocalhostError {
    LocalhostError::LaunchFailed { errno: code as i32 }
}

fn io_failed(error: std::io::Error) -> LocalhostError {
    launch_failed(error.raw_os_error().unwrap_or(0) as u32)
}
