//! The editors' changes that take more than one library call: removing a
//! Project or Service (stopped first, logs removed after), moving a Project
//! to another Execution Target, importing a Project file over another, and
//! saving the whole Environment editor. The apps make them on this
//! computer, and `machine::relay` makes them on another Machine for its app,
//! so both behave the same.
//!
//! Also what a folder picker shows of a folder (`list_directory`), for a
//! Machine whose folders no system dialog here can browse.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::{expand_tilde, home_directory};
use crate::{
    Id, Key, KeyClash, LocalhostError, Origin, ProfileChanges, Project, ProjectChanges, ProjectDocument,
    ProjectLibrary, Service, Targets, VariableInput,
};

/// One Environment Profile as the Environment editor saves it; `profile_id`
/// is `None` for a new one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ProfileInput {
    pub profile_id: Option<Id>,
    pub name: String,
    pub variables: Vec<VariableInput>,
}

/// Edits the Project. One moved to another Execution Target has its
/// Services stopped first, and their logs on the old one removed after.
pub async fn edit_project(
    library: &ProjectLibrary,
    targets: &Targets,
    project_id: Id,
    changes: ProjectChanges,
    origin: Origin,
) -> Result<Project, LocalhostError> {
    let project = library.project(project_id)?;
    let moves = changes.execution_target.as_ref().is_some_and(|target| *target != project.execution_target);
    if moves {
        for service in &project.services {
            targets.stop(service.id, origin).await?;
        }
    }
    let edited = library.edit_project(project_id, changes, origin)?;
    if moves {
        for service in &project.services {
            targets.remove_logs(&project.execution_target, service.id).await?;
        }
    }
    Ok(edited)
}

/// Stops the Project's Services, then removes it and their logs. The
/// folder is not touched.
pub async fn remove_project(
    library: &ProjectLibrary,
    targets: &Targets,
    project_id: Id,
    origin: Origin,
) -> Result<(), LocalhostError> {
    let project = library.project(project_id)?;
    for service in &project.services {
        targets.stop(service.id, origin).await?;
    }
    library.remove_project(project_id, origin)?;
    for service in &project.services {
        targets.remove_logs(&project.execution_target, service.id).await?;
    }
    Ok(())
}

/// Stops the Service, then removes it and its logs.
pub async fn remove_service(
    library: &ProjectLibrary,
    targets: &Targets,
    service_id: Id,
    origin: Origin,
) -> Result<(), LocalhostError> {
    let (project, _) = library.service(service_id)?;
    targets.stop(service_id, origin).await?;
    library.remove_service(service_id, origin)?;
    targets.remove_logs(&project.execution_target, service_id).await?;
    Ok(())
}

/// Imports the Project file. On Replace, Services the file no longer has
/// are stopped first, and their logs removed after; all of them if the
/// Project moves to another Execution Target.
pub async fn import_project(
    library: &ProjectLibrary,
    targets: &Targets,
    document: &ProjectDocument,
    on_key_clash: Option<KeyClash>,
    origin: Origin,
) -> Result<Project, LocalhostError> {
    let replaced = match on_key_clash {
        Some(KeyClash::Replace) => clashing(&library.projects(), document),
        _ => None,
    };
    let moves = |existing: &Project| existing.execution_target != *document.execution_target();
    if let Some(existing) = &replaced {
        let stopped: Vec<&Service> =
            if moves(existing) { existing.services.iter().collect() } else { removed_by(existing, document).collect() };
        for service in stopped {
            targets.stop(service.id, origin).await?;
        }
    }
    let project = library.import_project(document, on_key_clash, origin)?;
    if let Some(existing) = &replaced {
        let gone = |service: &&Service| moves(existing) || project.service(service.id).is_none();
        for service in existing.services.iter().filter(gone) {
            targets.remove_logs(&existing.execution_target, service.id).await?;
        }
    }
    Ok(project)
}

/// The Project among `projects` that has the document's Key, which an
/// import would replace or keep beside it.
pub fn clashing(projects: &[Project], document: &ProjectDocument) -> Option<Project> {
    projects.iter().find(|project| project.key == *document.key()).cloned()
}

/// The existing Project's Services that the file doesn't have: Replace
/// removes them.
pub fn removed_by<'a>(existing: &'a Project, document: &ProjectDocument) -> impl Iterator<Item = &'a Service> {
    let kept: HashSet<Key> = document.services().iter().map(|service| service.key.clone()).collect();
    existing.services.iter().filter(move |service| !kept.contains(&service.key))
}

/// Saves the Environment editor: the Project's variables, its Profiles
/// (added, edited, removed), then which one is active (an index into
/// `profiles`, or none).
pub fn save_environment(
    library: &ProjectLibrary,
    project_id: Id,
    variables: Vec<VariableInput>,
    profiles: Vec<ProfileInput>,
    active: Option<u32>,
    origin: Origin,
) -> Result<(), LocalhostError> {
    let project = library.project(project_id)?;
    let changes = ProjectChanges { variables: Some(variables), ..Default::default() };
    library.edit_project(project_id, changes, origin)?;
    let kept: HashSet<Id> = profiles.iter().filter_map(|profile| profile.profile_id).collect();
    for profile in project.profiles.iter().filter(|profile| !kept.contains(&profile.id)) {
        library.remove_profile(profile.id, origin)?;
    }
    let mut active_id = None;
    for (index, edit) in profiles.into_iter().enumerate() {
        let id = match edit.profile_id {
            Some(id) => {
                let changes = ProfileChanges { name: Some(edit.name), variables: Some(edit.variables) };
                library.edit_profile(id, changes, origin)?.id
            }
            None => library.add_profile(project_id, &edit.name, edit.variables, origin)?.id,
        };
        if active == Some(index as u32) {
            active_id = Some(id);
        }
    }
    library.set_active_profile(active_id, project_id, origin)
}

// MARK: Folders

/// A folder as a picker shows it: where it is, its parent, and what it holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DirectoryListing {
    /// Absolute and standardized.
    pub path: String,
    /// `None` at the root.
    pub parent: Option<String>,
    /// The user's home folder, where a picker starts.
    pub home: String,
    /// Folders first, then files; each sorted by name, ignoring case.
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DirectoryEntry {
    pub name: String,
    pub is_directory: bool,
}

/// What the folder at `path` (the home folder when `None`; `~` is expanded)
/// holds: its folders, and with `include_files` its files too. Names
/// starting with a dot are left out unless `show_hidden`.
pub fn list_directory(
    path: Option<&str>,
    show_hidden: bool,
    include_files: bool,
) -> Result<DirectoryListing, LocalhostError> {
    let home = home_directory();
    let raw = path.map(str::trim).filter(|path| !path.is_empty());
    let folder = match raw {
        Some(raw) => PathBuf::from(expand_tilde(raw)),
        None => home.clone(),
    };
    let shown = folder.display().to_string();
    if !folder.is_absolute() {
        return Err(LocalhostError::InvalidProjectPath(raw.unwrap_or_default().to_string()));
    }
    let folder = crate::paths::standardize(&folder);
    let unreadable = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => {
            LocalhostError::ProjectFolderMissing(shown.clone())
        }
        _ => LocalhostError::FolderUnreadable { path: shown.clone(), reason: reason(&error) },
    };
    if !folder.is_dir() {
        return Err(LocalhostError::ProjectFolderMissing(shown));
    }
    let mut entries: Vec<DirectoryEntry> = std::fs::read_dir(&folder)
        .map_err(unreadable)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            // A link to a folder is one to open.
            let is_directory = entry.path().is_dir();
            (is_directory || include_files).then_some(DirectoryEntry { name, is_directory })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_directory.cmp(&a.is_directory).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(DirectoryListing {
        path: folder.display().to_string(),
        parent: folder.parent().map(|parent| parent.display().to_string()),
        home: home.display().to_string(),
        entries,
    })
}

/// The system's reason, without Rust's "(os error 13)".
fn reason(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error") {
        Some(end) => text[..end].to_string(),
        None => text,
    }
}

/// A Project file's text, read for an import's review. Larger than any
/// Project file is refused rather than read.
pub fn read_project_file(path: &str) -> Result<String, LocalhostError> {
    const LIMIT: u64 = 1024 * 1024;
    let path = expand_tilde(path.trim());
    let failure = |reason: String| LocalhostError::PersistenceFailed { path: path.clone(), reason };
    let size = std::fs::metadata(&path).map_err(|error| failure(reason(&error)))?.len();
    if size > LIMIT {
        return Err(failure("it is too large to be a Project file".into()));
    }
    std::fs::read_to_string(&path).map_err(|error| failure(reason(&error)))
}

/// Whether `path` is a folder: an imported Project's may not be on this one.
pub fn folder_exists(path: &str) -> bool {
    Path::new(&expand_tilde(path)).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_and_hides_dot_names() {
        let root = tempfile::tempdir().unwrap();
        for folder in ["web", "Api", ".git"] {
            std::fs::create_dir(root.path().join(folder)).unwrap();
        }
        std::fs::write(root.path().join("shop.yaml"), "localhost: 1\n").unwrap();
        let path = root.path().to_str().unwrap();

        let names = |listing: DirectoryListing| listing.entries.into_iter().map(|entry| entry.name).collect::<Vec<_>>();
        assert_eq!(names(list_directory(Some(path), false, false).unwrap()), ["Api", "web"]);
        assert_eq!(names(list_directory(Some(path), true, true).unwrap()), [".git", "Api", "web", "shop.yaml"]);

        let listing = list_directory(Some(&format!("{path}/web/..")), false, false).unwrap();
        assert_eq!(listing.path, crate::paths::standardize(root.path()).display().to_string());
        assert!(listing.parent.is_some());
    }

    #[test]
    fn says_why_a_folder_cant_be_listed() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone");
        assert!(matches!(list_directory(missing.to_str(), false, false), Err(LocalhostError::ProjectFolderMissing(_))));
        assert!(matches!(
            list_directory(Some("relative/path"), false, false),
            Err(LocalhostError::InvalidProjectPath(_))
        ));
        assert!(list_directory(None, false, false).is_ok(), "starts at home");

        #[cfg(unix)]
        if !is_root() {
            use std::os::unix::fs::PermissionsExt;
            let locked = root.path().join("locked");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            let error = list_directory(locked.to_str(), false, false).unwrap_err();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                matches!(&error, LocalhostError::FolderUnreadable { reason, .. } if reason == "Permission denied"),
                "{error:?}"
            );
        }
    }

    #[cfg(unix)]
    fn is_root() -> bool {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "0")
    }
}
