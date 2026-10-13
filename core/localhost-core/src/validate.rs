//! Field validation and normalization shared by Project and Service edits
//! and Project files.

use std::path::Path;

use crate::LocalhostError;
use crate::paths::{expand_tilde, standardize};

pub(crate) fn name(raw: &str) -> Result<String, LocalhostError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(LocalhostError::InvalidName);
    }
    Ok(name.to_string())
}

/// An absolute path (a leading `~` is expanded), standardized.
pub(crate) fn project_path(raw: &str) -> Result<String, LocalhostError> {
    let expanded = expand_tilde(raw.trim());
    if !expanded.starts_with('/') {
        return Err(LocalhostError::InvalidProjectPath(raw.to_string()));
    }
    Ok(standardize(Path::new(&expanded)).to_string_lossy().into_owned())
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
