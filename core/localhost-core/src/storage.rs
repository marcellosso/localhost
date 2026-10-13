//! Reads and writes Project Configuration: one JSON file per Project at
//! `<root>/projects/<PROJECT-ID>.json` (ADR-0004), in the Swift core's
//! schema 1, so either can read what the other wrote.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Id, LocalhostError, Project};

pub(crate) const SCHEMA_VERSION: i64 = 1;

/// A Project file that couldn't be loaded at launch, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadableProjectFile {
    pub path: String,
    /// Where the file was moved aside to; `None` if it was left in place
    /// (written by a newer localhost) or couldn't be moved.
    pub moved_to: Option<String>,
    pub error: LocalhostError,
}

pub(crate) struct ProjectStorage {
    projects_directory: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileRef<'a> {
    schema_version: i64,
    project: &'a Project,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    #[allow(dead_code)]
    schema_version: i64,
    project: Project,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Version {
    schema_version: i64,
}

impl ProjectStorage {
    pub(crate) fn new(root: &Path) -> ProjectStorage {
        ProjectStorage { projects_directory: root.join("projects") }
    }

    pub(crate) fn file_path(&self, id: Id) -> PathBuf {
        self.projects_directory.join(format!("{id}.json"))
    }

    /// Every Project that can be loaded, plus the files that can't. A file
    /// written by a newer localhost is left in place for it; any other
    /// unreadable file (damaged, or a second copy of a Project or its Key) is
    /// moved aside as `<name>.unreadable`, so it is kept but never loaded or
    /// overwritten. Only an unreadable folder fails the whole load.
    pub(crate) fn load_all(&self) -> Result<(Vec<Project>, Vec<UnreadableProjectFile>), LocalhostError> {
        self.create_directory_if_needed()?;
        let entries = fs::read_dir(&self.projects_directory).map_err(|error| self.folder_failure(error))?;
        let mut file_names: Vec<String> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| self.folder_failure(error))?;
            if let Ok(name) = entry.file_name().into_string()
                && name.ends_with(".json")
            {
                file_names.push(name);
            }
        }

        let mut loaded: Vec<(PathBuf, Project)> = Vec::new();
        let mut unreadable = Vec::new();
        for name in file_names {
            let path = self.projects_directory.join(&name);
            match load(&path) {
                Ok(project) => loaded.push((path, project)),
                Err(error @ LocalhostError::UnsupportedSchemaVersion { .. }) => {
                    unreadable.push(UnreadableProjectFile { path: display(&path), moved_to: None, error });
                }
                Err(error) => {
                    unreadable.push(UnreadableProjectFile { path: display(&path), moved_to: move_aside(&path), error });
                }
            }
        }

        // Of two copies, the file saves write to (`<id>.json`) wins, then the
        // first by name, so the outcome doesn't depend on the folder's order.
        loaded.sort_by(|(a_path, a), (b_path, b)| {
            let a_is_own = *a_path == self.file_path(a.id);
            let b_is_own = *b_path == self.file_path(b.id);
            b_is_own.cmp(&a_is_own).then_with(|| a_path.file_name().cmp(&b_path.file_name()))
        });
        let mut projects: Vec<Project> = Vec::new();
        for (path, project) in loaded {
            let error = if let Some(original) = projects.iter().find(|p| p.id == project.id) {
                Some(LocalhostError::PersistenceFailed {
                    path: display(&path),
                    reason: format!("It is a second copy of the project “{}”.", original.name),
                })
            } else if projects.iter().any(|p| p.key == project.key) {
                Some(LocalhostError::DuplicateProjectKey(project.key.clone()))
            } else {
                None
            };
            match error {
                Some(error) => {
                    unreadable.push(UnreadableProjectFile { path: display(&path), moved_to: move_aside(&path), error })
                }
                None => projects.push(project),
            }
        }
        Ok((projects, unreadable))
    }

    /// Writes the Project's file atomically: a reader sees the old file or the
    /// new one, never part of it.
    pub(crate) fn save(&self, project: &Project) -> Result<(), LocalhostError> {
        self.create_directory_if_needed()?;
        let path = self.file_path(project.id);
        let failure =
            |error: io::Error| LocalhostError::PersistenceFailed { path: display(&path), reason: error.to_string() };
        let file = FileRef { schema_version: SCHEMA_VERSION, project };
        // `Value` objects keep their keys sorted, like the Swift core's files.
        let value = serde_json::to_value(&file).expect("a Project always encodes");
        let mut data = serde_json::to_vec_pretty(&value).expect("a Value always encodes");
        data.push(b'\n');

        let temporary = self.projects_directory.join(format!(".{}.tmp-{}", project.id, Id::new()));
        let result = (|| {
            let mut handle = fs::File::create(&temporary)?;
            handle.write_all(&data)?;
            handle.sync_all()?;
            fs::rename(&temporary, &path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(failure(error));
        }
        Ok(())
    }

    pub(crate) fn delete(&self, id: Id) -> Result<(), LocalhostError> {
        let path = self.file_path(id);
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(LocalhostError::PersistenceFailed { path: display(&path), reason: error.to_string() })
            }
            _ => Ok(()),
        }
    }

    fn create_directory_if_needed(&self) -> Result<(), LocalhostError> {
        fs::create_dir_all(&self.projects_directory).map_err(|error| self.folder_failure(error))
    }

    fn folder_failure(&self, error: io::Error) -> LocalhostError {
        LocalhostError::PersistenceFailed { path: display(&self.projects_directory), reason: error.to_string() }
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn load(path: &Path) -> Result<Project, LocalhostError> {
    let failure = |reason: String| LocalhostError::PersistenceFailed { path: display(path), reason };
    let data = fs::read(path).map_err(|error| failure(error.to_string()))?;
    let version: Version = decode(&data).map_err(failure)?;
    if version.schema_version != SCHEMA_VERSION {
        return Err(LocalhostError::UnsupportedSchemaVersion { version: version.schema_version, path: display(path) });
    }
    let file: File = decode(&data).map_err(failure)?;
    Ok(file.project)
}

/// Decodes JSON, explaining in a sentence why it doesn't decode.
fn decode<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(data);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let inner = error.inner();
        if !inner.is_data() {
            return "It is damaged: it isn’t valid JSON.".to_string();
        }
        let path = error.path().to_string();
        let field = |name: &str| if path == "." { name.to_string() } else { format!("{path}.{name}") };
        // serde_json's message, without " at line 1 column 2".
        let message = inner.to_string();
        let message = message.rsplit_once(" at line ").map_or(message.as_str(), |(message, _)| message);
        if let Some(name) = message.strip_prefix("missing field `").and_then(|rest| rest.strip_suffix('`')) {
            format!("It is damaged: “{}” is missing.", field(name))
        } else if message.starts_with("invalid type:") {
            format!("It is damaged: “{path}” has the wrong type.")
        } else {
            format!("It is damaged: “{path}”: {message}")
        }
    })
}

/// Renames the file to `<name>.unreadable` (or `.unreadable-2`, …), never
/// replacing an earlier one. Returns the new path, or `None` if it couldn't
/// be moved.
fn move_aside(path: &Path) -> Option<String> {
    for suffix in 1.. {
        let destination = PathBuf::from(format!(
            "{}{}",
            display(path),
            if suffix == 1 { ".unreadable".into() } else { format!(".unreadable-{suffix}") }
        ));
        if destination.symlink_metadata().is_ok() {
            continue;
        }
        return fs::rename(path, &destination).ok().map(|_| display(&destination));
    }
    None
}
