use std::collections::BTreeMap;

/// Builds the environment of a Run. Later layers override earlier ones:
/// base (the app's environment) → variables (Project → Service → active
/// Environment Profile) → `PORT`, plus output hints that keep colour and
/// unbuffered output when writing to pipes.
///
/// The command runs through a login shell, so the user's login profile is
/// applied on top when the shell starts. On Windows, where no
/// shell does that, the base is the user's environment read fresh from the
/// registry at every start, and names match regardless of case.
pub struct RunEnvironment;

impl RunEnvironment {
    pub const OUTPUT_HINTS: [(&'static str, &'static str); 3] =
        [("FORCE_COLOR", "1"), ("CLICOLOR_FORCE", "1"), ("PYTHONUNBUFFERED", "1")];

    pub fn merged(
        base: &BTreeMap<String, String>,
        variables: &BTreeMap<String, String>,
        port: Option<u16>,
    ) -> BTreeMap<String, String> {
        // Loader variables injected into the app (e.g. by a debugger) must
        // not leak into the user's commands.
        let mut environment: BTreeMap<String, String> = base
            .iter()
            .filter(|(name, _)| !name.starts_with("DYLD_") && !name.starts_with("__XPC_"))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        for (name, value) in Self::OUTPUT_HINTS {
            set(&mut environment, name, value.into());
        }
        for (name, value) in variables {
            set(&mut environment, name, value.clone());
        }
        if let Some(port) = port {
            set(&mut environment, "PORT", port.to_string());
        }
        environment
    }

    /// What a Run's environment starts from, given the configured base (see
    /// `ServiceManagerOptions`). On Windows: the user's environment as a new
    /// logon would see it, read now, with `configured` on top. Elsewhere,
    /// `configured` itself.
    pub fn base(configured: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        if !cfg!(windows) {
            return configured.clone();
        }
        let mut environment = Self::current();
        for (name, value) in configured {
            set(&mut environment, name, value.clone());
        }
        environment
    }

    /// The environment this process was started with, less what an AppImage
    /// added for the app itself (see `without_appimage`). On Windows, the
    /// user's environment as a new logon would see it (user and system
    /// variables from the registry, `Path` merged), read now.
    pub fn current() -> BTreeMap<String, String> {
        #[cfg(windows)]
        if let Some(environment) = login_environment() {
            return environment;
        }
        let environment = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        Self::without_appimage(environment)
    }

    /// An AppImage's runtime points `PATH`, `LD_LIBRARY_PATH`,
    /// `XDG_DATA_DIRS`, the GTK and GIO module paths and more into its own
    /// mount, which goes away when the app quits. The user's commands (and
    /// the terminal or browser the app opens) must not load from there: drop
    /// every list entry under `$APPDIR`, then variables left with nothing,
    /// and the AppImage's own variables.
    pub fn without_appimage(mut environment: BTreeMap<String, String>) -> BTreeMap<String, String> {
        let Some(appdir) = environment.get("APPDIR").filter(|_| environment.contains_key("APPIMAGE")).cloned() else {
            return environment;
        };
        let appdir = appdir.trim_end_matches('/').to_string();
        let is_inside = |entry: &str| entry == appdir || entry.starts_with(&format!("{appdir}/"));
        for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            environment.remove(name);
        }
        environment.retain(|_, value| {
            if !value.contains(&appdir) {
                return true;
            }
            let kept = value.split(':').filter(|entry| !is_inside(entry)).collect::<Vec<_>>().join(":");
            *value = kept;
            !value.split(':').all(str::is_empty)
        });
        // The GTK hook sets it to `$APPIMAGE_GTK_THEME`, usually empty.
        if environment.get("GTK_THEME").is_some_and(String::is_empty) {
            environment.remove("GTK_THEME");
        }
        environment
    }

    /// The user's shell: `$SHELL`, or the platform's default (zsh on macOS, sh
    /// elsewhere). On Windows, always `cmd.exe`: `%ComSpec%`, else
    /// the one in `%SystemRoot%\System32`.
    pub fn shell(environment: &BTreeMap<String, String>) -> String {
        if cfg!(windows) {
            let system_root = value(environment, "SystemRoot").unwrap_or(r"C:\Windows");
            return match value(environment, "ComSpec") {
                Some(shell) => shell.into(),
                None => format!(r"{system_root}\System32\cmd.exe"),
            };
        }
        match environment.get("SHELL") {
            Some(shell) if !shell.is_empty() => shell.clone(),
            _ if cfg!(target_os = "macos") => "/bin/zsh".into(),
            _ => "/bin/sh".into(),
        }
    }
}

/// Whether two variable names are the same: on Windows regardless of case.
fn same_name(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

fn set(environment: &mut BTreeMap<String, String>, name: &str, value: String) {
    if cfg!(windows) {
        environment.retain(|existing, _| !same_name(existing, name));
    }
    environment.insert(name.into(), value);
}

/// A variable's non-empty value.
fn value<'a>(environment: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    environment
        .iter()
        .find(|(existing, value)| same_name(existing, name) && !value.is_empty())
        .map(|(_, value)| value.as_str())
}

/// `CreateEnvironmentBlock` for this process's user, without inheriting
/// this process's environment.
#[cfg(windows)]
fn login_environment() -> Option<BTreeMap<String, String>> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut block: *mut std::ffi::c_void = std::ptr::null_mut();
        if CreateEnvironmentBlock(&mut block, token.as_raw_handle(), 0) == 0 {
            return None;
        }
        // `name=value` strings, each NUL-terminated, ending with an empty one.
        let mut environment = BTreeMap::new();
        let mut entry = block as *const u16;
        loop {
            let length = (0..).take_while(|&offset| *entry.add(offset) != 0).count();
            if length == 0 {
                break;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(entry, length));
            // Names starting with `=` are cmd's per-drive directories.
            if let Some((name, value)) = text.split_once('=').filter(|(name, _)| !name.is_empty()) {
                environment.insert(name.to_string(), value.to_string());
            }
            entry = entry.add(length + 1);
        }
        DestroyEnvironmentBlock(block);
        Some(environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn variables_override_the_base_environment() {
        let environment = RunEnvironment::merged(
            &map(&[("PATH", "/usr/bin"), ("PORT", "1")]),
            &map(&[("PORT", "3000"), ("API_URL", "http://localhost:8000")]),
            None,
        );
        assert_eq!(environment["PATH"], "/usr/bin");
        assert_eq!(environment["PORT"], "3000");
        assert_eq!(environment["API_URL"], "http://localhost:8000");
    }

    #[test]
    fn adds_the_output_hints() {
        let environment = RunEnvironment::merged(&map(&[]), &map(&[]), None);
        assert_eq!(environment["FORCE_COLOR"], "1");
        assert_eq!(environment["CLICOLOR_FORCE"], "1");
        assert_eq!(environment["PYTHONUNBUFFERED"], "1");
    }

    #[test]
    fn a_variable_can_override_an_output_hint() {
        let environment = RunEnvironment::merged(&map(&[]), &map(&[("FORCE_COLOR", "0")]), None);
        assert_eq!(environment["FORCE_COLOR"], "0");
    }

    #[test]
    fn port_overrides_every_variable() {
        let environment = RunEnvironment::merged(&map(&[("PORT", "1")]), &map(&[("PORT", "2")]), Some(4567));
        assert_eq!(environment["PORT"], "4567");
    }

    #[test]
    fn drops_loader_variables_injected_into_the_app() {
        let environment = RunEnvironment::merged(
            &map(&[("DYLD_INSERT_LIBRARIES", "/x.dylib"), ("__XPC_DYLD_LIBRARY_PATH", "/y"), ("HOME", "/Users/dev")]),
            &map(&[]),
            None,
        );
        assert!(!environment.contains_key("DYLD_INSERT_LIBRARIES"));
        assert!(!environment.contains_key("__XPC_DYLD_LIBRARY_PATH"));
        assert_eq!(environment["HOME"], "/Users/dev");
    }

    #[test]
    #[cfg(not(windows))]
    fn the_shell_comes_from_shell_falling_back_to_the_platform_default() {
        let fallback = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" };
        assert_eq!(RunEnvironment::shell(&map(&[("SHELL", "/opt/homebrew/bin/fish")])), "/opt/homebrew/bin/fish");
        assert_eq!(RunEnvironment::shell(&map(&[])), fallback);
        assert_eq!(RunEnvironment::shell(&map(&[("SHELL", "")])), fallback);
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_the_shell_is_cmd() {
        assert_eq!(
            RunEnvironment::shell(&map(&[("ComSpec", r"D:\Win\cmd.exe"), ("SHELL", "/bin/bash")])),
            r"D:\Win\cmd.exe"
        );
        assert_eq!(RunEnvironment::shell(&map(&[("SystemRoot", r"D:\Win")])), r"D:\Win\System32\cmd.exe");
        assert_eq!(RunEnvironment::shell(&map(&[])), r"C:\Windows\System32\cmd.exe");
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_names_match_regardless_of_case() {
        let environment = RunEnvironment::merged(
            &map(&[("Path", r"C:\Windows"), ("port", "1")]),
            &map(&[("PATH", r"C:\tools")]),
            Some(4567),
        );
        assert_eq!(environment, {
            let mut expected = map(&[("PATH", r"C:\tools"), ("PORT", "4567")]);
            expected.extend(map(&RunEnvironment::OUTPUT_HINTS));
            expected
        });
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_the_base_is_the_login_environment_with_the_configured_variables_on_top() {
        let base = RunEnvironment::base(&map(&[("LOCALHOST_TEST", "1"), ("path", r"C:\only")]));
        assert_eq!(base["LOCALHOST_TEST"], "1");
        assert_eq!(base.iter().filter(|(name, _)| name.eq_ignore_ascii_case("PATH")).count(), 1);
        assert_eq!(base["path"], r"C:\only");
        assert!(base.keys().any(|name| name.eq_ignore_ascii_case("SystemRoot")), "{base:?}");
    }

    #[test]
    #[cfg(not(windows))]
    fn elsewhere_the_base_is_the_configured_environment() {
        let configured = map(&[("PATH", "/usr/bin")]);
        assert_eq!(RunEnvironment::base(&configured), configured);
    }

    #[test]
    fn drops_what_an_appimage_added() {
        let environment = RunEnvironment::without_appimage(map(&[
            ("APPDIR", "/tmp/.mount_localhXYZ"),
            ("APPIMAGE", "/home/me/localhost-app.AppImage"),
            ("OWD", "/home/me"),
            ("PATH", "/tmp/.mount_localhXYZ/usr/bin:/home/me/.cargo/bin:/usr/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_localhXYZ/usr/lib:"),
            ("XDG_DATA_DIRS", "/tmp/.mount_localhXYZ/usr/share:/usr/share:/usr/local/share"),
            ("GTK_PATH", "/tmp/.mount_localhXYZ/usr/lib/gtk-3.0"),
            ("GTK_THEME", ""),
            ("NOTES", "/tmp/.mount_localhXYZother/kept"),
            ("HOME", "/home/me"),
        ]));
        assert_eq!(
            environment,
            map(&[
                ("PATH", "/home/me/.cargo/bin:/usr/bin"),
                ("XDG_DATA_DIRS", "/usr/share:/usr/local/share"),
                ("NOTES", "/tmp/.mount_localhXYZother/kept"),
                ("HOME", "/home/me"),
            ])
        );
    }

    #[test]
    fn outside_an_appimage_nothing_is_dropped() {
        let original = map(&[("APPDIR", "/opt/thing"), ("PATH", "/opt/thing/bin:/usr/bin"), ("GTK_THEME", "")]);
        assert_eq!(RunEnvironment::without_appimage(original.clone()), original);
    }
}
