//! Launching a command line in its own session with `posix_spawn`, rather
//! than `fork`, which is safer and cheaper inside a large app process.

use std::ffi::CString;
use std::os::fd::{FromRawFd, OwnedFd};
use std::ptr;

use crate::LocalhostError;
use crate::process::LaunchConfiguration;

/// `<spawn.h>`'s `POSIX_SPAWN_SETSID`, which the `libc` crate lacks on macOS.
#[cfg(target_os = "macos")]
const POSIX_SPAWN_SETSID: libc::c_int = 0x0400;
#[cfg(target_os = "linux")]
const POSIX_SPAWN_SETSID: libc::c_int = libc::POSIX_SPAWN_SETSID as libc::c_int;

pub(crate) struct Pipe {
    pub(crate) read: OwnedFd,
    pub(crate) write: OwnedFd,
}

/// A pipe whose ends are close-on-exec, so no other spawned process inherits them.
pub(crate) fn pipe() -> Result<Pipe, LocalhostError> {
    let mut fds = [0; 2];
    #[cfg(target_os = "linux")]
    let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(target_os = "linux"))]
    let result = unsafe {
        let result = libc::pipe(fds.as_mut_ptr());
        if result == 0 {
            libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC);
        }
        result
    };
    if result != 0 {
        return Err(LocalhostError::LaunchFailed { errno: errno() });
    }
    Ok(unsafe { Pipe { read: OwnedFd::from_raw_fd(fds[0]), write: OwnedFd::from_raw_fd(fds[1]) } })
}

pub(crate) fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Spawns `<shell> -l -c <command line>` in a new session with stdin on
/// /dev/null, stdout/stderr on the given pipes, default signal handling, and
/// no other inherited file descriptors. Returns the shell's pid, which is
/// also its process group and session id.
pub(crate) fn spawn(
    configuration: &LaunchConfiguration,
    stdout: &OwnedFd,
    stderr: &OwnedFd,
) -> Result<libc::pid_t, LocalhostError> {
    use std::os::fd::AsRawFd;

    let c_string = |s: &str| CString::new(s).map_err(|_| LocalhostError::LaunchFailed { errno: libc::EINVAL });
    let shell = c_string(&configuration.shell)?;
    let arguments: Vec<CString> = [configuration.shell.as_str(), "-l", "-c", &configuration.command_line]
        .into_iter()
        .map(c_string)
        .collect::<Result<_, _>>()?;
    let environment: Vec<CString> = configuration
        .environment
        .iter()
        .filter(|(name, _)| !name.is_empty() && !name.contains('='))
        .map(|(name, value)| c_string(&format!("{name}={value}")))
        .collect::<Result<_, _>>()?;
    let directory = c_string(&configuration.working_directory.to_string_lossy())?;
    let mut argv: Vec<*mut libc::c_char> = arguments.iter().map(|s| s.as_ptr() as *mut _).collect();
    argv.push(ptr::null_mut());
    let mut envp: Vec<*mut libc::c_char> = environment.iter().map(|s| s.as_ptr() as *mut _).collect();
    envp.push(ptr::null_mut());

    unsafe {
        let mut attributes: libc::posix_spawnattr_t = std::mem::zeroed();
        libc::posix_spawnattr_init(&mut attributes);
        let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        libc::posix_spawn_file_actions_init(&mut actions);

        let flags = POSIX_SPAWN_SETSID | libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK;
        #[cfg(target_os = "macos")]
        let flags = flags | libc::POSIX_SPAWN_CLOEXEC_DEFAULT;
        libc::posix_spawnattr_setflags(&mut attributes, flags as _);
        let mut all_signals: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut all_signals);
        libc::posix_spawnattr_setsigdefault(&mut attributes, &all_signals);
        let mut no_signals: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut no_signals);
        libc::posix_spawnattr_setsigmask(&mut attributes, &no_signals);

        libc::posix_spawn_file_actions_addopen(&mut actions, 0, c"/dev/null".as_ptr(), libc::O_RDONLY, 0);
        libc::posix_spawn_file_actions_adddup2(&mut actions, stdout.as_raw_fd(), 1);
        libc::posix_spawn_file_actions_adddup2(&mut actions, stderr.as_raw_fd(), 2);
        // Linux has no CLOEXEC_DEFAULT: close everything else explicitly.
        // musl (the static `localhostd` in WSL) has no `addclosefrom_np`:
        // there Rust and tokio open every descriptor close-on-exec already.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        libc::posix_spawn_file_actions_addclosefrom_np(&mut actions, 3);
        libc::posix_spawn_file_actions_addchdir_np(&mut actions, directory.as_ptr());

        let mut pid: libc::pid_t = 0;
        let result = libc::posix_spawn(&mut pid, shell.as_ptr(), &actions, &attributes, argv.as_ptr(), envp.as_ptr());
        libc::posix_spawn_file_actions_destroy(&mut actions);
        libc::posix_spawnattr_destroy(&mut attributes);
        match result {
            0 => Ok(pid),
            libc::ENOENT => Err(LocalhostError::ShellNotFound(configuration.shell.clone())),
            errno => Err(LocalhostError::LaunchFailed { errno }),
        }
    }
}
