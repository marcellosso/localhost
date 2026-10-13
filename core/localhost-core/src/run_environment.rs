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

    /// The environment this process was started with.
    pub fn current() -> BTreeMap<String, String> {
        std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect()
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
}
