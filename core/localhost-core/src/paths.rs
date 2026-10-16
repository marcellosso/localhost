use std::path::{Component, Path, PathBuf};

/// Where localhost keeps its data on this platform (`docs/cross-platform.md`):
/// `~/Library/Application Support/localhost` on macOS, `%APPDATA%\localhost`
/// on Windows, and `$XDG_DATA_HOME/localhost` (by default
/// `~/.local/share/localhost`) elsewhere.
pub fn default_storage_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        return home_directory().join("Library/Application Support/localhost");
    }
    if cfg!(windows) {
        return match std::env::var_os("APPDATA") {
            Some(data) if Path::new(&data).is_absolute() => PathBuf::from(data).join("localhost"),
            _ => home_directory().join("AppData").join("Roaming").join("localhost"),
        };
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(data) if Path::new(&data).is_absolute() => PathBuf::from(data).join("localhost"),
        _ => home_directory().join(".local/share/localhost"),
    }
}

pub(crate) fn home_directory() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => std::env::home_dir().unwrap_or_else(|| PathBuf::from("/")),
    }
}

/// `path` with a leading `~` (alone or followed by `/`) replaced by the home folder.
pub(crate) fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return home_directory().to_string_lossy().into_owned();
    }
    match path.strip_prefix("~/") {
        Some(rest) => home_directory().join(rest).to_string_lossy().into_owned(),
        None => path.to_string(),
    }
}

/// `path` with the home folder (alone or followed by `/`) replaced by `~`.
pub(crate) fn abbreviate_tilde(path: &str) -> String {
    let home = home_directory().to_string_lossy().into_owned();
    if home == "/" {
        return path.to_string();
    }
    match path.strip_prefix(home.as_str()) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// `path` with `.` and `..` resolved without touching the file system, and
/// no trailing separator: `/tmp/app/../shared/` → `/tmp/shared`.
pub(crate) fn standardize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() || result.as_os_str().is_empty() {
                    result.push(Component::RootDir);
                }
            }
            other => result.push(other),
        }
    }
    if result.as_os_str().is_empty() { PathBuf::from(".") } else { result }
}

/// An absolute POSIX path with `.` and `..` resolved and no trailing `/`,
/// as text, so it is the same on every platform (a WSL distro's paths are
/// POSIX on Windows too): `/tmp/app/../shared/` → `/tmp/shared`.
pub(crate) fn standardize_posix(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standardizes_posix_paths_as_text() {
        assert_eq!(standardize_posix("/tmp/app/../shared/tools/"), "/tmp/shared/tools");
        assert_eq!(standardize_posix("/tmp/app/./."), "/tmp/app");
        assert_eq!(standardize_posix("/../.."), "/");
        assert_eq!(standardize_posix("//home//me"), "/home/me");
    }

    #[test]
    fn standardizes_lexically() {
        assert_eq!(standardize(Path::new("/tmp/app/../shared/tools/")), PathBuf::from("/tmp/shared/tools"));
        assert_eq!(standardize(Path::new("/tmp/app/./.")), PathBuf::from("/tmp/app"));
        assert_eq!(standardize(Path::new("/../..")), PathBuf::from("/"));
    }

    #[test]
    #[cfg(unix)]
    fn expands_a_leading_tilde_only() {
        let home = home_directory().to_string_lossy().into_owned();
        assert_eq!(expand_tilde("~/Code/a"), format!("{home}/Code/a"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/tmp/~/a"), "/tmp/~/a");
        assert_eq!(expand_tilde("~other/a"), "~other/a");
    }

    #[test]
    #[cfg(unix)]
    fn abbreviates_the_home_folder_only() {
        let home = home_directory().to_string_lossy().into_owned();
        assert_eq!(abbreviate_tilde(&format!("{home}/Code/a")), "~/Code/a");
        assert_eq!(abbreviate_tilde(&home), "~");
        assert_eq!(abbreviate_tilde(&format!("{home}other/a")), format!("{home}other/a"));
        assert_eq!(abbreviate_tilde("/tmp/a"), "/tmp/a");
    }
}
