//! The shared core's transport on Windows: a named pipe named after the
//! storage root. Pipe names are one namespace for every user of the
//! machine, so the pipe lets only this user in (its DACL), the core takes
//! the name only while it is free (`first_pipe_instance`), and each end
//! checks that the other's process runs as this user: a client never sends
//! Secrets to a pipe another user made first.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};
use tokio::signal::windows::{ctrl_close, ctrl_logoff, ctrl_shutdown};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY, GENERIC_ALL, HANDLE};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, AddAccessAllowedAce, GetLengthSid, GetTokenInformation, InitializeAcl,
    InitializeSecurityDescriptor, PSID, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SetSecurityDescriptorDacl,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, GetCurrentProcess, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::hash;
use crate::RunEnvironment;

pub struct Listener {
    name: String,
    /// The instance the next client connects to.
    next: NamedPipeServer,
}

pub(super) type Reader = ReadHalf<NamedPipeClient>;
pub(super) type Writer = WriteHalf<NamedPipeClient>;

/// `SECURITY_DESCRIPTOR_REVISION`, from a module windows-sys gates behind
/// a feature this crate doesn't otherwise need.
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;

/// How long a client waits while every instance of the pipe is taken: the
/// core makes the next one as soon as a client connects.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// `\\.\pipe\localhostd-<hash>`, from the storage root regardless of case,
/// as Windows paths match.
pub fn pipe_name(storage_root: &Path) -> String {
    let root = storage_root.to_string_lossy().to_lowercase();
    format!(r"\\.\pipe\localhostd-{:016x}", hash(Path::new(&root)))
}

pub(super) async fn connect(storage_root: &Path) -> io::Result<(Reader, Writer)> {
    let name = pipe_name(storage_root);
    let deadline = tokio::time::Instant::now() + BUSY_TIMEOUT;
    let client = loop {
        match ClientOptions::new().open(&name) {
            Ok(client) => break client,
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(error) => return Err(error),
        }
    };
    let mut server = 0;
    // SAFETY: a plain call on a handle `client` owns.
    let found = unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut server) } != 0;
    if !found || !runs_as_this_user(server) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "its pipe is held by another user"));
    }
    Ok(tokio::io::split(client))
}

/// Starts the core without a window, with the user's environment as a new
/// logon sees it, and out of the client's job: an agent's runtime (Node's,
/// for one) puts its children in a job that ends with it.
pub(super) fn spawn_detached(
    mut daemon: std::process::Command,
    storage_root: &Path,
) -> io::Result<std::process::Child> {
    // Not the client's folder, which it would keep from being removed.
    daemon.current_dir(storage_root).env_clear().envs(RunEnvironment::current());
    match daemon.creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        // A job that doesn't allow it: the core then ends with the client's job.
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            daemon.creation_flags(CREATE_NO_WINDOW).spawn()
        }
        spawned => spawned,
    }
}

/// Listens on the pipe of `storage_root`. Fails if anyone already has it.
/// Call it holding the lock.
pub fn bind(storage_root: &Path) -> io::Result<Listener> {
    let name = pipe_name(storage_root);
    let next = create(&name, true)?;
    Ok(Listener { name, next })
}

/// The next connection; `None` if it isn't this user's.
pub(super) async fn accept(
    listener: &mut Listener,
) -> io::Result<Option<(ReadHalf<NamedPipeServer>, WriteHalf<NamedPipeServer>)>> {
    if let Err(error) = listener.next.connect().await {
        listener.next = create(&listener.name, false)?;
        return Err(error);
    }
    let next = match create(&listener.name, false) {
        Ok(next) => next,
        Err(error) => {
            let _ = listener.next.disconnect();
            return Err(error);
        }
    };
    let connected = std::mem::replace(&mut listener.next, next);
    let mut client = 0;
    // SAFETY: a plain call on a handle `connected` owns.
    let found = unsafe { GetNamedPipeClientProcessId(connected.as_raw_handle(), &mut client) } != 0;
    Ok((found && runs_as_this_user(client)).then(|| tokio::io::split(connected)))
}

/// An instance of the pipe that only this user may open, nor from another
/// machine.
fn create(name: &str, is_first: bool) -> io::Result<NamedPipeServer> {
    let user = this_user()?;
    let sid = user.as_ptr() as PSID;
    // SAFETY: `sid` is a valid SID for the whole function; the ACL and the
    // descriptor are sized for it and outlive the call that reads them.
    unsafe {
        let acl_length =
            size_of::<ACL>() + size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>() + GetLengthSid(sid) as usize;
        let mut acl = vec![0_u32; acl_length.div_ceil(4)];
        let acl_pointer = acl.as_mut_ptr().cast::<ACL>();
        let mut descriptor: SECURITY_DESCRIPTOR = std::mem::zeroed();
        let descriptor_pointer = (&raw mut descriptor).cast();
        if InitializeAcl(acl_pointer, size_of_val(acl.as_slice()) as u32, ACL_REVISION) == 0
            || AddAccessAllowedAce(acl_pointer, ACL_REVISION, GENERIC_ALL, sid) == 0
            || InitializeSecurityDescriptor(descriptor_pointer, SECURITY_DESCRIPTOR_REVISION) == 0
            || SetSecurityDescriptorDacl(descriptor_pointer, 1, acl_pointer, 0) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor_pointer,
            bInheritHandle: 0,
        };
        ServerOptions::new()
            .first_pipe_instance(is_first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attributes).cast())
    }
}

/// This process's user's SID, read once.
fn this_user() -> io::Result<&'static [u32]> {
    static USER: OnceLock<Result<Vec<u32>, i32>> = OnceLock::new();
    // SAFETY: the pseudo-handle of this process needs no closing.
    let user =
        USER.get_or_init(|| user_of(unsafe { GetCurrentProcess() }).map_err(|error| error.raw_os_error().unwrap_or(0)));
    user.as_deref().map_err(|&code| io::Error::from_raw_os_error(code))
}

/// Whether process `pid` runs as this user: a client can run any command
/// as the user whose core it reaches.
fn runs_as_this_user(pid: u32) -> bool {
    // SAFETY: a plain call; the handle is owned from here on.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    matches!((user_of(process.as_raw_handle()), this_user()), (Ok(theirs), Ok(ours)) if theirs == ours)
}

/// The SID of the user `process` runs as, padded to whole `u32`s, which
/// SIDs need to be aligned to.
fn user_of(process: HANDLE) -> io::Result<Vec<u32>> {
    // SAFETY: plain calls on handles owned here, with buffers of the size
    // `GetTokenInformation` asks for, aligned for `TOKEN_USER`'s pointer.
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut length = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, ptr::null_mut(), 0, &mut length);
        let mut buffer = vec![0_u64; (length as usize).div_ceil(8)];
        if GetTokenInformation(token.as_raw_handle(), TokenUser, buffer.as_mut_ptr().cast(), length, &mut length) == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let sid_length = GetLengthSid(sid) as usize;
        let mut copy = vec![0_u32; sid_length.div_ceil(4)];
        ptr::copy_nonoverlapping(sid.cast::<u8>(), copy.as_mut_ptr().cast::<u8>(), sid_length);
        Ok(copy)
    }
}

/// Completes when its console closes, or the user logs off or shuts down.
/// Not on Ctrl-C or Ctrl-Break: those are for the Runs on its console.
/// Never fails to start the core: without these, its Runs still end with
/// it, as their jobs close.
pub(super) fn told_to_end() -> io::Result<impl Future<Output = ()>> {
    let (mut close, mut logoff, mut shutdown) = (ctrl_close().ok(), ctrl_logoff().ok(), ctrl_shutdown().ok());
    Ok(async move {
        tokio::select! {
            Some(_) = async { close.as_mut()?.recv().await } => {}
            Some(_) = async { logoff.as_mut()?.recv().await } => {}
            Some(_) = async { shutdown.as_mut()?.recv().await } => {}
            else => std::future::pending().await,
        }
    })
}

/// A pipe goes with its last handle: nothing is left behind.
pub(super) fn clean_up(_storage_root: &Path) {}
