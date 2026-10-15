//! Field validation and normalization shared by Project and Service edits
//! and Project files.

use crate::paths::{expand_tilde, standardize_posix};
use crate::{ExecutionTarget, LocalhostError};

pub(crate) fn name(raw: &str) -> Result<String, LocalhostError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(LocalhostError::InvalidName);
    }
    Ok(name.to_string())
}

/// An absolute path on the target, standardized. A leading `~` is expanded
/// on this machine only: a distro's home folder isn't known here.
pub(crate) fn project_path(raw: &str, target: &ExecutionTarget) -> Result<String, LocalhostError> {
    let expanded = match target {
        ExecutionTarget::ThisMachine => expand_tilde(raw.trim()),
        ExecutionTarget::Wsl { .. } => raw.trim().to_string(),
    };
    if !expanded.starts_with('/') {
        return Err(LocalhostError::InvalidProjectPath(raw.to_string()));
    }
    Ok(standardize_posix(&expanded))
}

/// The target with its distro name trimmed. WSL distro names are letters,
/// digits, `.`, `-` and `_`.
pub(crate) fn execution_target(target: ExecutionTarget) -> Result<ExecutionTarget, LocalhostError> {
    match target {
        ExecutionTarget::ThisMachine => Ok(ExecutionTarget::ThisMachine),
        ExecutionTarget::Wsl { distro } => {
            let name = distro.trim();
            let is_valid = !name.is_empty()
                && name.chars().all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character));
            if !is_valid {
                return Err(LocalhostError::InvalidDistroName(distro));
            }
            Ok(ExecutionTarget::Wsl { distro: name.to_string() })
        }
    }
}

/// A path relative to the Project, e.g. `web` or `../shared`; empty means `.`.
pub(crate) fn working_directory(raw: &str) -> Result<String, LocalhostError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('~') {
        return Err(LocalhostError::InvalidWorkingDirectory(raw.to_string()));
    }
    let components: Vec<&str> = trimmed.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    Ok(if components.is_empty() { ".".into() } else { components.join("/") })
}

pub(crate) fn command_line(raw: &str) -> Result<String, LocalhostError> {
    let command_line = raw.trim();
    if command_line.is_empty() {
        return Err(LocalhostError::EmptyCommandLine);
    }
    Ok(command_line.to_string())
}

pub(crate) fn port(port: Option<i64>) -> Result<Option<u16>, LocalhostError> {
    match port {
        None => Ok(None),
        Some(port) if (1..=65_535).contains(&port) => Ok(Some(port as u16)),
        Some(port) => Err(LocalhostError::InvalidPort(port)),
    }
}

/// An absolute http(s) URL with a host; blank means none.
pub(crate) fn url(raw: Option<&str>) -> Result<Option<String>, LocalhostError> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    let invalid = || LocalhostError::InvalidUrl(trimmed.to_string());
    // Only `scheme://host…` counts: the parser alone would accept `http:host`.
    let (scheme, rest) = trimmed.split_once("://").ok_or_else(invalid)?;
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") || rest.is_empty() {
        return Err(invalid());
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| invalid())?;
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(invalid());
    }
    Ok(Some(trimmed.to_string()))
}

/// Variable names must be portable shell names: `[A-Za-z_][A-Za-z0-9_]*`.
pub(crate) fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_paths_are_absolute_and_only_this_machines_expand_a_tilde() {
        let wsl = ExecutionTarget::Wsl { distro: "Ubuntu".into() };
        assert_eq!(project_path(" /home/me/app/../shop/ ", &wsl), Ok("/home/me/shop".into()));
        assert_eq!(project_path("~/shop", &wsl), Err(LocalhostError::InvalidProjectPath("~/shop".into())));
        assert!(project_path("~/shop", &ExecutionTarget::ThisMachine).unwrap().ends_with("/shop"));
        assert!(project_path("shop", &ExecutionTarget::ThisMachine).is_err());
    }

    #[test]
    fn distro_names_are_trimmed_and_checked() {
        let wsl = |distro: &str| ExecutionTarget::Wsl { distro: distro.into() };
        assert_eq!(execution_target(wsl(" Ubuntu-24.04 ")), Ok(wsl("Ubuntu-24.04")));
        assert_eq!(execution_target(wsl("")), Err(LocalhostError::InvalidDistroName("".into())));
        assert_eq!(execution_target(wsl("my distro")), Err(LocalhostError::InvalidDistroName("my distro".into())));
    }

    #[test]
    fn urls_need_http_and_a_host() {
        assert_eq!(url(Some(" http://localhost:3000/admin ")), Ok(Some("http://localhost:3000/admin".into())));
        assert_eq!(url(Some("HTTPS://example.com")), Ok(Some("HTTPS://example.com".into())));
        assert_eq!(url(Some("  ")), Ok(None));
        assert_eq!(url(None), Ok(None));
        for bad in ["localhost:3000", "ftp://example.com", "http://", "http:example.com", "mailto:a@b.c"] {
            assert_eq!(url(Some(bad)), Err(LocalhostError::InvalidUrl(bad.into())), "{bad}");
        }
    }

    #[test]
    fn ports_are_1_to_65535() {
        assert_eq!(port(Some(1)), Ok(Some(1)));
        assert_eq!(port(Some(65_535)), Ok(Some(65_535)));
        assert_eq!(port(Some(0)), Err(LocalhostError::InvalidPort(0)));
        assert_eq!(port(Some(65_536)), Err(LocalhostError::InvalidPort(65_536)));
    }

    #[test]
    fn variable_names_are_portable_shell_names() {
        for good in ["A", "_OK_1", "api_key", "PORT"] {
            assert!(is_valid_variable_name(good), "{good}");
        }
        for bad in ["", "1PORT", "MY-VAR", "MY VAR", "ÉTÉ"] {
            assert!(!is_valid_variable_name(bad), "{bad}");
        }
    }
}
