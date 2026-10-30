//! The bundled `localhost` command on the user's `PATH`, so it works in a
//! terminal too: a link in `~/.local/bin` on macOS and Linux, the app's
//! folder on the user's `Path` on Windows. Agents don't need it: their
//! configuration names the command by its full path.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::AgentSetup;
use super::files::{self, display};
use crate::LocalhostError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CommandLineState {
    /// localhost put it there; Remove takes it away.
    Added,
    NotAdded,
    /// It is there without localhost's help, e.g. the Linux packages' `/usr/bin/localhost`.
    AlreadyOnPath,
    /// Another file is where the link would go; localhost leaves it alone.
    Taken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommandLineStatus {
    pub state: CommandLineState,
    /// The link (macOS, Linux), or the folder added to the user's `Path` (Windows).
    pub path: String,
    /// The command it runs.
    pub command: String,
    /// What is the case and what to do next.
    pub summary: String,
}

impl AgentSetup {
    pub fn command_line(&self) -> CommandLineStatus {
        let target = self.link_path();
        let shown = display(&target, &self.home);
        let mut status = CommandLineStatus {
            state: CommandLineState::NotAdded,
            path: target.display().to_string(),
            command: self.command.display().to_string(),
            summary: String::new(),
        };
        let folder = self.command.parent().map(Path::to_path_buf).unwrap_or_default();
        let on_path = |folder: &Path| self.terminal_path().is_some_and(|path| contains(&path, folder));
        if cfg!(windows) {
            if platform::user_path().is_some_and(|path| contains(&path, &folder)) {
                status.state = CommandLineState::Added;
                status.summary = format!("{shown} is on your PATH: new terminals run `localhost`.");
            } else if on_path(&folder) {
                status.state = CommandLineState::AlreadyOnPath;
                status.summary = "`localhost` is on your PATH already.".into();
            } else {
                status.summary = format!("Adds {shown} to your PATH, so terminals can run `localhost`.");
            }
            return status;
        }
        if on_path(&folder) {
            status.state = CommandLineState::AlreadyOnPath;
            status.path = self.command.display().to_string();
            status.summary = format!("`localhost` is on your PATH already, as {}.", display(&self.command, &self.home));
            return status;
        }
        let link_folder = target.parent().map(Path::to_path_buf).unwrap_or_default();
        let link_folder_shown = display(&link_folder, &self.home);
        match std::fs::read_link(&target) {
            Ok(destination) if destination == self.command => {
                status.state = CommandLineState::Added;
                status.summary = if on_path(&link_folder) {
                    "New terminals run `localhost`.".into()
                } else {
                    format!(
                        "Linked, but {link_folder_shown} isn’t on your PATH. Add it in your shell profile: export PATH=\"$HOME/.local/bin:$PATH\""
                    )
                };
            }
            _ if files::exists(&target) || std::fs::symlink_metadata(&target).is_ok() => {
                status.state = CommandLineState::Taken;
                status.summary = format!("{shown} is another file, which localhost leaves alone.");
            }
            _ => status.summary = format!("Adds a link at {shown} to the command inside the app."),
        }
        status
    }

    /// Puts the command on the user's `PATH`.
    pub fn add_command_to_path(&self) -> Result<CommandLineStatus, LocalhostError> {
        self.check_command()?;
        match self.command_line().state {
            CommandLineState::Added | CommandLineState::AlreadyOnPath => {}
            CommandLineState::Taken => return Err(LocalhostError::CommandPathTaken(self.command_line().path)),
            CommandLineState::NotAdded if cfg!(windows) => {
                let folder = self.command.parent().unwrap_or(Path::new(""));
                platform::add_to_user_path(folder)?;
            }
            CommandLineState::NotAdded => {
                let link = self.link_path();
                if let Some(folder) = link.parent() {
                    std::fs::create_dir_all(folder).map_err(|error| files::failure(folder, error))?;
                }
                platform::symlink(&self.command, &link).map_err(|error| files::failure(&link, error))?;
            }
        }
        Ok(self.command_line())
    }

    /// Takes away what `add_command_to_path` added, and nothing else.
    pub fn remove_command_from_path(&self) -> Result<CommandLineStatus, LocalhostError> {
        if self.command_line().state == CommandLineState::Added {
            if cfg!(windows) {
                platform::remove_from_user_path(self.command.parent().unwrap_or(Path::new("")))?;
            } else {
                let link = self.link_path();
                std::fs::remove_file(&link).map_err(|error| files::failure(&link, error))?;
            }
        }
        Ok(self.command_line())
    }

    fn link_path(&self) -> PathBuf {
        if cfg!(windows) {
            return self.command.parent().map(Path::to_path_buf).unwrap_or_default();
        }
        files::join(&self.home, &[".local", "bin", "localhost"])
    }

    fn terminal_path(&self) -> Option<String> {
        self.terminal_path.get_or_init(platform::terminal_path).clone()
    }
}

/// Whether the `PATH`-style list `path` holds `folder`.
fn contains(path: &str, folder: &Path) -> bool {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let normalize = |entry: &str| {
        let entry = entry.trim().trim_end_matches(['/', '\\']).to_string();
        if cfg!(windows) { entry.to_lowercase() } else { entry }
    };
    let folder = normalize(&folder.to_string_lossy());
    !folder.is_empty() && path.split(separator).any(|entry| normalize(entry) == folder)
}

#[cfg(unix)]
mod platform {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::LocalhostError;

    pub(super) fn symlink(command: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(command, link)
    }

    /// `PATH` in a new terminal: what the login shell sets, which an app
    /// started from the Dock or a desktop launcher doesn't inherit. Waits
    /// up to 5 seconds for a slow profile.
    pub(super) fn terminal_path() -> Option<String> {
        let environment = crate::RunEnvironment::current();
        let shell = crate::RunEnvironment::shell(&environment);
        let mut child = Command::new(shell)
            .args(["-l", "-c", "printf %s \"$PATH\""])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return std::env::var("PATH").ok();
                }
            }
        }
        let output = child.wait_with_output().ok()?;
        String::from_utf8(output.stdout).ok().filter(|path| !path.is_empty())
    }

    pub(super) fn user_path() -> Option<String> {
        None
    }

    pub(super) fn add_to_user_path(_folder: &Path) -> Result<(), LocalhostError> {
        Ok(())
    }

    pub(super) fn remove_from_user_path(_folder: &Path) -> Result<(), LocalhostError> {
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ, RegCloseKey, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    use crate::LocalhostError;

    pub(super) fn symlink(_command: &Path, _link: &Path) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    /// The user's and the system's `Path` as a new logon gets them.
    pub(super) fn terminal_path() -> Option<String> {
        let environment = crate::RunEnvironment::current();
        environment.iter().find(|(name, _)| name.eq_ignore_ascii_case("Path")).map(|(_, value)| value.clone())
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn failure(reason: impl ToString) -> LocalhostError {
        LocalhostError::PersistenceFailed {
            path: r"HKEY_CURRENT_USER\Environment\Path".into(),
            reason: reason.to_string(),
        }
    }

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn open(access: u32) -> Result<Key, LocalhostError> {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide("Environment").as_ptr(), 0, access, &mut key) };
        if status != ERROR_SUCCESS {
            return Err(failure(std::io::Error::from_raw_os_error(status as i32)));
        }
        Ok(Key(key))
    }

    /// The user's own `Path` (`HKCU\Environment`), unexpanded; `None` if it has none.
    pub(super) fn user_path() -> Option<String> {
        let key = open(KEY_QUERY_VALUE).ok()?;
        let name = wide("Path");
        let mut kind = 0;
        let mut size = 0u32;
        let status = unsafe {
            RegQueryValueExW(key.0, name.as_ptr(), std::ptr::null(), &mut kind, std::ptr::null_mut(), &mut size)
        };
        if status != ERROR_SUCCESS || (kind != REG_SZ && kind != REG_EXPAND_SZ) {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let status = unsafe {
            RegQueryValueExW(key.0, name.as_ptr(), std::ptr::null(), &mut kind, buffer.as_mut_ptr().cast(), &mut size)
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let length = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..length]))
    }

    fn set_user_path(value: &str) -> Result<(), LocalhostError> {
        let key = open(KEY_SET_VALUE)?;
        let data = wide(value);
        let status = unsafe {
            RegSetValueExW(
                key.0,
                wide("Path").as_ptr(),
                0,
                REG_EXPAND_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(failure(std::io::Error::from_raw_os_error(status as i32)));
        }
        // New terminals started from Explorer read the change.
        let environment = wide("Environment");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            )
        };
        Ok(())
    }

    pub(super) fn add_to_user_path(folder: &Path) -> Result<(), LocalhostError> {
        let current = user_path().unwrap_or_default();
        let folder = folder.display().to_string();
        let mut entries: Vec<&str> = current.split(';').filter(|entry| !entry.is_empty()).collect();
        entries.push(&folder);
        set_user_path(&entries.join(";"))
    }

    pub(super) fn remove_from_user_path(folder: &Path) -> Result<(), LocalhostError> {
        let Some(current) = user_path() else { return Ok(()) };
        let kept: Vec<&str> =
            current.split(';').filter(|entry| !entry.is_empty() && !super::contains(entry, folder)).collect();
        set_user_path(&kept.join(";"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_folder_in_a_path_list() {
        if cfg!(windows) {
            assert!(contains(
                r"C:\Windows;C:\Users\Me\AppData\Local\localhost\",
                Path::new(r"c:\users\me\appdata\local\localhost")
            ));
        } else {
            assert!(contains("/usr/bin:/home/me/.local/bin/", Path::new("/home/me/.local/bin")));
            assert!(!contains("/usr/bin", Path::new("/usr/bin/x")));
            assert!(!contains("", Path::new("")));
        }
    }
}
