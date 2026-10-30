//! Reading and replacing agents' files without losing what is in them.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::LocalhostError;

pub(super) fn failure(path: &Path, error: impl ToString) -> LocalhostError {
    LocalhostError::PersistenceFailed { path: path.to_string_lossy().into_owned(), reason: error.to_string() }
}

/// The file's text, or `None` when there is no file.
pub(super) fn read(path: &Path) -> Result<Option<String>, LocalhostError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failure(path, error)),
    }
}

/// Replaces `path` with `text` in one step, so the agent never reads half a
/// file. A symlink (a dotfiles repository) is followed, not replaced, and
/// the file keeps its permissions.
pub(super) fn write(path: &Path, text: &str) -> Result<(), LocalhostError> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = target.parent().ok_or_else(|| failure(path, "it has no folder"))?;
    fs::create_dir_all(folder).map_err(|error| failure(folder, error))?;
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = folder.join(format!(".{name}.localhost-{}", std::process::id()));
    let result = (|| {
        fs::write(&temporary, text)?;
        if let Ok(metadata) = fs::metadata(&target) {
            fs::set_permissions(&temporary, metadata.permissions())?;
        }
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| failure(path, error))
}

/// Copies `path` to `backup` unless a backup is there already: the first
/// one is the file as it was before localhost ever changed it.
pub(super) fn back_up(path: &Path, backup: &Path) -> Result<(), LocalhostError> {
    if backup.exists() || !path.exists() {
        return Ok(());
    }
    if let Some(folder) = backup.parent() {
        fs::create_dir_all(folder).map_err(|error| failure(folder, error))?;
    }
    fs::copy(path, backup).map(|_| ()).map_err(|error| failure(backup, error))
}

/// Removes `file`, then its folder if that is left empty.
pub(super) fn remove_with_folder(file: &Path) -> Result<(), LocalhostError> {
    match fs::remove_file(file) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(failure(file, error)),
    }
    if let Some(folder) = file.parent()
        && fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(folder);
    }
    Ok(())
}

/// `path` for people: under the home folder as `~/…`.
pub(super) fn display(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if !rest.as_os_str().is_empty() => {
            let separator = if cfg!(windows) { "\\" } else { "/" };
            format!("~{separator}{}", rest.display())
        }
        _ => path.display().to_string(),
    }
}

pub(super) fn exists(path: &Path) -> bool {
    path.try_exists().unwrap_or(false)
}

pub(super) fn join(home: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(home.to_path_buf(), |path, part| path.join(part))
}
