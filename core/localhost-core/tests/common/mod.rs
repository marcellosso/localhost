#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use localhost_core::{Key, MemorySecretStore, ProjectLibrary};
use tempfile::TempDir;

/// A storage root in a temporary folder, removed when dropped, with Secrets
/// kept in memory so tests never touch the user's real store.
pub struct Fixture {
    pub root: TempDir,
    pub secrets: Arc<MemorySecretStore>,
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture { root: tempfile::tempdir().unwrap(), secrets: Arc::new(MemorySecretStore::new()) }
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn library(&self) -> ProjectLibrary {
        ProjectLibrary::open(self.path(), self.secrets.clone()).unwrap()
    }

    pub fn projects_directory(&self) -> PathBuf {
        self.path().join("projects")
    }

    pub fn project_file(&self, id: localhost_core::Id) -> PathBuf {
        self.projects_directory().join(format!("{id}.json"))
    }

    /// Writes a file into the projects folder, as a crash or another tool might.
    pub fn write_project_file(&self, name: &str, contents: &str) -> PathBuf {
        std::fs::create_dir_all(self.projects_directory()).unwrap();
        let path = self.projects_directory().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }
}

pub fn key(raw: &str) -> Key {
    Key::new(raw).unwrap()
}
