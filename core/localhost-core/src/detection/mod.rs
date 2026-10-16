//! Scans a Project's folder for common project files and suggests Services.
//!
//! Scanning only reads: it never writes into the folder and makes no network
//! calls. It looks at the root, every folder up to two levels deep (skipping
//! dependency, build and hidden folders) and the folders detectors declare,
//! such as workspace packages. It reads the file system, so clients call it
//! off their UI thread.

mod cargo;
mod compose;
mod manifest;
mod node;
mod python;
mod syntax;

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use cargo::CargoDetector;
pub use compose::ComposeDetector;
pub use node::NodeDetector;
pub use python::PythonDetector;
pub use syntax::CommandSyntax;

use crate::paths::{expand_tilde, standardize};
use crate::{Key, NewService, Project};

/// A Service that scanning a Project's folder suggests. It is only a
/// proposal: the user reviews and edits it before anything is saved, and an
/// unchecked suggestion is never added.
///
/// Suggestions are plain command lines like any Service. Detectors only shape
/// them, so a suggested Service is never inherently "a Node service".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ServiceSuggestion {
    pub name: String,
    pub key: Key,
    /// Relative to the Project's path; `.` is the Project root.
    pub working_directory: String,
    pub command_line: String,
    pub port: Option<u16>,
    /// Off when the command line can't take another port (e.g. a port fixed
    /// in a Compose file or written into the script).
    pub port_can_change: bool,
    /// Whether the review step checks it by default.
    pub is_recommended: bool,
    /// The file the suggestion comes from, relative to the Project's path,
    /// e.g. `apps/web/package.json`.
    pub found_in: String,
    /// What in that file it comes from, e.g. `"dev": vite`.
    pub detail: String,
}

impl ServiceSuggestion {
    fn new(
        name: impl Into<String>,
        working_directory: &str,
        command_line: impl Into<String>,
        is_recommended: bool,
        found_in: String,
        detail: impl Into<String>,
    ) -> ServiceSuggestion {
        let name = name.into();
        ServiceSuggestion {
            key: suggestion_key(&name),
            name,
            working_directory: working_directory.to_string(),
            command_line: command_line.into(),
            port: None,
            port_can_change: true,
            is_recommended,
            found_in,
            detail: detail.into(),
        }
    }
}

impl From<ServiceSuggestion> for NewService {
    fn from(suggestion: ServiceSuggestion) -> NewService {
        NewService {
            name: suggestion.name,
            key: suggestion.key,
            working_directory: suggestion.working_directory,
            command_line: suggestion.command_line,
            port: suggestion.port.map(i64::from),
            url: None,
            port_can_change: suggestion.port_can_change,
            is_excluded_from_start_all: false,
        }
    }
}

/// A Key for a suggestion's name, `service` when the name has no usable characters.
fn suggestion_key(name: &str) -> Key {
    Key::suggested_from(name).unwrap_or_else(|| Key::new("service").expect("a valid key"))
}

/// Recognises one kind of project file and suggests Services for it.
///
/// Detectors are small and independent: each looks at one folder at a time
/// and returns plain command lines. None of them makes a Service
/// runtime-specific.
pub trait ServiceDetector: Send + Sync {
    /// Suggestions for the files directly inside `directory`.
    fn suggestions(&self, directory: &ScannedDirectory) -> Vec<ServiceSuggestion>;

    /// Folders (relative to the Project) this detector knows belong to the
    /// Project beyond the ones the scanner finds itself, e.g. workspace
    /// packages declared at the Project root.
    fn declared_directories(&self, _root: &ScannedDirectory) -> Vec<String> {
        Vec::new()
    }
}

/// Folders that hold dependencies, build output or tooling, never Services.
const IGNORED_FOLDER_NAMES: [&str; 14] = [
    "node_modules",
    "bower_components",
    "vendor",
    "venv",
    "env",
    "__pycache__",
    "site-packages",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "Pods",
    "DerivedData",
];

const MAXIMUM_DIRECTORY_COUNT: usize = 400;

pub struct ProjectScanner {
    detectors: Vec<Box<dyn ServiceDetector>>,
    syntax: CommandSyntax,
}

impl Default for ProjectScanner {
    /// Compose, Node, Python and Cargo.
    fn default() -> Self {
        ProjectScanner::new(vec![
            Box::new(ComposeDetector),
            Box::new(NodeDetector),
            Box::new(PythonDetector),
            Box::new(CargoDetector),
        ])
    }
}

impl ProjectScanner {
    pub fn new(detectors: Vec<Box<dyn ServiceDetector>>) -> ProjectScanner {
        ProjectScanner { detectors, syntax: CommandSyntax::HERE }
    }

    /// Suggests command lines for another shell than this machine's.
    pub fn with_syntax(mut self, syntax: CommandSyntax) -> ProjectScanner {
        self.syntax = syntax;
        self
    }

    /// Suggestions for the Project folder at `path`, folder by folder (root
    /// first), with Keys made unique. A folder with nothing recognisable
    /// yields none.
    pub fn scan_folder(&self, path: &str) -> Vec<ServiceSuggestion> {
        making_keys_unique(self.suggestions_for_folder(path), HashSet::new())
    }

    /// Suggestions for an existing Project's folder, leaving out those its
    /// Services already cover (same working directory and command line) and
    /// avoiding the Keys they use.
    pub fn scan_project(&self, project: &Project) -> Vec<ServiceSuggestion> {
        let new: Vec<ServiceSuggestion> = self
            .suggestions_for_folder(&project.path)
            .into_iter()
            .filter(|suggestion| {
                !project.services.iter().any(|service| {
                    service.working_directory == suggestion.working_directory
                        && service.command_line == suggestion.command_line
                })
            })
            .collect();
        let taken = project.services.iter().map(|service| service.key.to_string()).collect();
        making_keys_unique(new, taken)
    }

    fn suggestions_for_folder(&self, path: &str) -> Vec<ServiceSuggestion> {
        let root = ScannedDirectory {
            root: standardize(Path::new(&expand_tilde(path))),
            path: ".".into(),
            syntax: self.syntax,
        };
        let mut suggestions = Vec::new();
        for directory in self.directories(&root) {
            for detector in &self.detectors {
                suggestions.extend(detector.suggestions(&directory));
            }
        }
        suggestions
    }

    fn directories(&self, root: &ScannedDirectory) -> Vec<ScannedDirectory> {
        let mut paths: BTreeSet<String> = BTreeSet::new();
        let mut level = vec![root.clone()];
        for _ in 0..2 {
            level = level
                .iter()
                .flat_map(|directory| {
                    directory
                        .subdirectory_names()
                        .into_iter()
                        .filter(|name| is_scannable(name))
                        .map(|name| directory.subdirectory(&name))
                })
                .collect();
            paths.extend(level.iter().map(|directory| directory.path.clone()));
        }
        for detector in &self.detectors {
            paths.extend(detector.declared_directories(root).iter().filter_map(|path| normalized(path)));
        }
        let mut directories = vec![root.clone()];
        directories.extend(
            paths
                .into_iter()
                .take(MAXIMUM_DIRECTORY_COUNT)
                .map(|path| ScannedDirectory { root: root.root.clone(), path, syntax: root.syntax })
                .filter(ScannedDirectory::exists),
        );
        directories
    }
}

fn is_scannable(name: &str) -> bool {
    !name.starts_with('.') && !IGNORED_FOLDER_NAMES.contains(&name)
}

/// Later suggestions whose Key is already taken get `-2`, `-3`, ….
fn making_keys_unique(suggestions: Vec<ServiceSuggestion>, mut taken: HashSet<String>) -> Vec<ServiceSuggestion> {
    suggestions
        .into_iter()
        .map(|mut suggestion| {
            let base = suggestion.key.to_string();
            let mut candidate = base.clone();
            let mut number = 2;
            while taken.contains(&candidate) {
                let suffix = format!("-{number}");
                candidate = format!("{}{suffix}", &base[..base.len().min(Key::MAX_LENGTH - suffix.len())]);
                number += 1;
            }
            taken.insert(candidate.clone());
            if let Ok(key) = Key::new(candidate) {
                suggestion.key = key;
            }
            suggestion
        })
        .collect()
}

/// A relative folder path without `.` components, `None` if it leaves the Project.
fn normalized(path: &str) -> Option<String> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    if components.contains(&"..") || path.starts_with('/') || components.is_empty() {
        return None;
    }
    Some(components.join("/"))
}

/// One folder of a Project being scanned, with read-only access to its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedDirectory {
    /// The Project's root folder.
    pub root: PathBuf,
    /// Relative to the root; `.` is the root itself.
    pub path: String,
    /// The shell the suggested command lines are written for.
    pub syntax: CommandSyntax,
}

/// Files larger than this are treated as unreadable.
const MAXIMUM_FILE_SIZE: u64 = 1_000_000;

impl ScannedDirectory {
    pub fn full_path(&self) -> PathBuf {
        if self.is_root() { self.root.clone() } else { self.root.join(&self.path) }
    }

    pub fn is_root(&self) -> bool {
        self.path == "."
    }

    /// The folder's own name; the Project folder's name for the root.
    pub fn name(&self) -> String {
        self.full_path().file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    }

    fn exists(&self) -> bool {
        self.full_path().is_dir()
    }

    /// `name` relative to the Project root, e.g. `apps/web/package.json`.
    pub fn relative_path(&self, name: &str) -> String {
        if self.is_root() { name.to_string() } else { format!("{}/{name}", self.path) }
    }

    pub fn subdirectory(&self, name: &str) -> ScannedDirectory {
        ScannedDirectory { root: self.root.clone(), path: self.relative_path(name), syntax: self.syntax }
    }

    /// The folder this one is in, `None` for the root.
    pub fn parent(&self) -> Option<ScannedDirectory> {
        if self.is_root() {
            return None;
        }
        let path = self.path.rsplit_once('/').map_or(".", |(parent, _)| parent);
        Some(ScannedDirectory { root: self.root.clone(), path: path.to_string(), syntax: self.syntax })
    }

    /// This folder and every folder above it, up to the root.
    pub fn self_and_ancestors(&self) -> Vec<ScannedDirectory> {
        std::iter::successors(Some(self.clone()), ScannedDirectory::parent).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.full_path().join(name).exists()
    }

    /// The text of a file in this folder (`name` may contain `/`), `None` if
    /// it is missing, too large or not UTF-8.
    pub fn read(&self, name: &str) -> Option<String> {
        let path = self.full_path().join(name);
        if fs::metadata(&path).ok()?.len() > MAXIMUM_FILE_SIZE {
            return None;
        }
        String::from_utf8(fs::read(path).ok()?).ok()
    }

    /// The folders inside this one (not following links), sorted.
    pub fn subdirectory_names(&self) -> Vec<String> {
        self.entries(|metadata| metadata.is_dir(), false)
    }

    /// The files inside this one, sorted.
    pub fn file_names(&self) -> Vec<String> {
        self.entries(|metadata| metadata.is_file(), true)
    }

    fn entries(&self, keep: impl Fn(&fs::Metadata) -> bool, follow_links: bool) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.full_path()) else { return Vec::new() };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                let metadata =
                    if follow_links { fs::metadata(entry.path()) } else { fs::symlink_metadata(entry.path()) };
                metadata.is_ok_and(|metadata| keep(&metadata))
            })
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    /// Expands workspace-style globs (`apps/*`, `packages/**`, `tools/cli`,
    /// with `!` excludes) relative to this folder into folder paths relative
    /// to the Project root. `**` reaches at most four levels down.
    pub fn expanding_globs(&self, patterns: &[String]) -> Vec<String> {
        let mut included: Vec<String> = Vec::new();
        let mut excluded: Vec<String> = Vec::new();
        for raw in patterns {
            let (is_exclude, pattern) = match raw.strip_prefix('!') {
                Some(pattern) => (true, pattern),
                None => (false, raw.as_str()),
            };
            let components: Vec<&str> = pattern.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
            if components.contains(&"..") {
                continue;
            }
            if is_exclude {
                excluded.push(components.join("/"));
            } else {
                included.extend(self.matching(&components, 4).into_iter().map(|directory| directory.path));
            }
        }
        included.retain(|path| !excluded.iter().any(|pattern| glob_matches(pattern, &self.relative_to_self(path))));
        included
    }

    fn relative_to_self(&self, project_path: &str) -> String {
        if self.is_root() { project_path.to_string() } else { project_path[self.path.len() + 1..].to_string() }
    }

    fn matching(&self, components: &[&str], depth_left: usize) -> Vec<ScannedDirectory> {
        let Some((&first, rest)) = components.split_first() else { return vec![self.clone()] };
        let children = || self.subdirectory_names().into_iter().filter(|name| is_scannable(name));
        if first == "**" {
            if depth_left == 0 {
                return self.matching(rest, 0);
            }
            let mut found = self.matching(rest, depth_left);
            for child in children() {
                found.extend(self.subdirectory(&child).matching(components, depth_left - 1));
            }
            return found;
        }
        if first.contains('*') {
            return children()
                .filter(|name| wildcard_matches(first, name))
                .flat_map(|name| self.subdirectory(&name).matching(rest, depth_left))
                .collect();
        }
        let next = self.subdirectory(first);
        if next.exists() { next.matching(rest, depth_left) } else { Vec::new() }
    }
}

/// Whether a path glob (`**` for any number of folders) matches a relative path.
pub(crate) fn glob_matches(pattern: &str, path: &str) -> bool {
    fn matches(pattern: &[&str], path: &[&str]) -> bool {
        let Some((&first, rest)) = pattern.split_first() else { return path.is_empty() };
        if first == "**" {
            return matches(rest, path) || (!path.is_empty() && matches(pattern, &path[1..]));
        }
        match path.split_first() {
            Some((&component, path_rest)) if wildcard_matches(first, component) => matches(rest, path_rest),
            _ => false,
        }
    }
    let pattern: Vec<&str> = pattern.split('/').filter(|c| !c.is_empty()).collect();
    let path: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    matches(&pattern, &path)
}

/// Whether a single-component pattern with `*` wildcards matches a name.
pub(crate) fn wildcard_matches(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut remaining = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match remaining.find(part) {
            Some(index) => remaining = &remaining[index + part.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_globs_match_folders() {
        for (pattern, path, expected) in [
            ("apps/*", "apps/web", true),
            ("apps/*", "apps/web/src", false),
            ("packages/**", "packages/a/b", true),
            ("**/fixtures", "packages/x/fixtures", true),
            ("app-*", "app-web", true),
            ("app-*", "lib-web", false),
            ("*-*-*", "a-b-c", true),
            ("a*b*c", "abc", true),
            ("a*b*c", "acb", false),
        ] {
            assert_eq!(glob_matches(pattern, path), expected, "{pattern} {path}");
        }
    }

    #[test]
    fn normalizes_declared_folders_and_rejects_ones_outside_the_project() {
        assert_eq!(normalized("./apps//web/"), Some("apps/web".into()));
        assert_eq!(normalized("../elsewhere"), None);
        assert_eq!(normalized("/abs"), None);
        assert_eq!(normalized("."), None);
    }

    #[test]
    fn later_duplicate_keys_get_a_number() {
        let suggestion = |name: &str| ServiceSuggestion::new(name, ".", "x", true, "f".into(), "d");
        let unique = making_keys_unique(
            vec![suggestion("web"), suggestion("Web"), suggestion("web")],
            HashSet::from(["web-2".into()]),
        );
        let keys: Vec<String> = unique.iter().map(|s| s.key.to_string()).collect();
        assert_eq!(keys, ["web", "web-3", "web-4"]);
    }
}
