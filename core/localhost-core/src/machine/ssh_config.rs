//! The hosts the user has named in `~/.ssh/config`, to suggest as a
//! Machine's destination. Only read, never written. `ssh` itself decides
//! what each name means; this only lists them.

use std::path::{Path, PathBuf};

/// How deep `Include`s are followed, as `ssh` limits them.
const MAX_DEPTH: usize = 16;

/// The `Host` names of the user's `~/.ssh/config` and the files it
/// includes, in order, without patterns (`*`, `?`, `!`), which name no one
/// host.
pub fn ssh_config_hosts() -> Vec<String> {
    hosts_in(&crate::paths::home_directory().join(".ssh"))
}

/// The hosts of `ssh_folder`/config.
pub(crate) fn hosts_in(ssh_folder: &Path) -> Vec<String> {
    let mut hosts = Vec::new();
    read(&ssh_folder.join("config"), ssh_folder, 0, &mut hosts);
    hosts
}

fn read(file: &Path, ssh_folder: &Path, depth: usize, hosts: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else { return };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `Keyword value` or `Keyword=value`.
        let split = line.find(|character: char| character.is_whitespace() || character == '=').unwrap_or(line.len());
        let (keyword, rest) = line.split_at(split);
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
        if keyword.eq_ignore_ascii_case("host") {
            for name in words(rest) {
                // A pattern names no one host, and a destination has no spaces.
                let is_usable = !name.contains(['*', '?', '!']) && !name.contains(char::is_whitespace);
                if is_usable && !hosts.contains(&name) {
                    hosts.push(name);
                }
            }
        } else if keyword.eq_ignore_ascii_case("include") {
            for pattern in words(rest) {
                for included in expand(&pattern, ssh_folder) {
                    read(&included, ssh_folder, depth + 1, hosts);
                }
            }
        }
    }
}

/// The words of a value, some of them quoted.
fn words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut is_quoted = false;
    for character in value.chars() {
        match character {
            '"' => is_quoted = !is_quoted,
            character if character.is_whitespace() && !is_quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            character => word.push(character),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The files an `Include` names: relative to `~/.ssh`, `~` expanded, with
/// `*` and `?` in its last part, sorted as `ssh` reads them.
fn expand(pattern: &str, ssh_folder: &Path) -> Vec<PathBuf> {
    let path = match pattern.strip_prefix("~/") {
        Some(rest) => crate::paths::home_directory().join(rest),
        None if Path::new(pattern).is_absolute() => PathBuf::from(pattern),
        None => ssh_folder.join(pattern),
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else { return Vec::new() };
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let Some(folder) = path.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(|file| matches(name, file)))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

/// Whether `text` matches the glob `pattern` of `*` and `?`.
fn matches(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&character| character == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_named_hosts_following_includes() {
        let folder = tempfile::tempdir().unwrap();
        let ssh = folder.path();
        std::fs::create_dir_all(ssh.join("config.d")).unwrap();
        std::fs::write(
            ssh.join("config"),
            "# mine\nInclude config.d/*\nHost devbox lab\n  HostName 100.64.0.2\nHost *.internal !bastion\n\
             host=pi\nMatch host foo\nHost \"quoted box\" devbox\nInclude missing\nInclude config\n",
        )
        .unwrap();
        std::fs::write(ssh.join("config.d/10-work"), "Host work-vm\n  User me\nHost ?\n").unwrap();
        std::fs::write(ssh.join("config.d/20-home"), "HOST nas\n").unwrap();

        assert_eq!(hosts_in(ssh), ["work-vm", "nas", "devbox", "lab", "pi"]);
    }

    #[test]
    fn has_no_hosts_without_a_config() {
        assert!(hosts_in(&tempfile::tempdir().unwrap().path().join(".ssh")).is_empty());
    }

    #[test]
    fn matches_globs() {
        assert!(matches("*", "anything"));
        assert!(matches("*.conf", "work.conf"));
        assert!(!matches("*.conf", "work.conf.bak"));
        assert!(matches("h?st", "host"));
        assert!(!matches("h?st", "hoost"));
    }
}
