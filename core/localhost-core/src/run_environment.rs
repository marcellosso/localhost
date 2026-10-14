use std::collections::BTreeMap;

/// Builds the environment of a Run. Later layers override earlier ones:
/// base (the app's environment) → variables (Project → Service → active
/// Environment Profile) → `PORT`, plus output hints that keep colour and
/// unbuffered output when writing to pipes (ADR-0005).
///
/// The command runs through a login shell, so the user's login profile is
/// applied on top when the shell starts (ADR-0003).
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
            environment.insert(name.into(), value.into());
        }
        environment.extend(variables.iter().map(|(name, value)| (name.clone(), value.clone())));
        if let Some(port) = port {
            environment.insert("PORT".into(), port.to_string());
        }
        environment
    }

    /// The environment this process was started with, less what an AppImage
    /// added for the app itself (see `without_appimage`).
    pub fn current() -> BTreeMap<String, String> {
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

    /// The user's shell: `$SHELL`, or the platform's default (zsh on macOS, sh elsewhere).
    pub fn shell(environment: &BTreeMap<String, String>) -> String {
        match environment.get("SHELL") {
            Some(shell) if !shell.is_empty() => shell.clone(),
            _ if cfg!(target_os = "macos") => "/bin/zsh".into(),
            _ => "/bin/sh".into(),
        }
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
    fn the_shell_comes_from_shell_falling_back_to_the_platform_default() {
        let fallback = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" };
        assert_eq!(RunEnvironment::shell(&map(&[("SHELL", "/opt/homebrew/bin/fish")])), "/opt/homebrew/bin/fish");
        assert_eq!(RunEnvironment::shell(&map(&[])), fallback);
        assert_eq!(RunEnvironment::shell(&map(&[("SHELL", "")])), fallback);
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
