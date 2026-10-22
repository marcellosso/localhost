//! `localhost status <project>`: which of the Project's Services are running.
//!
//! The command runs in its own process, so it sees the Runs of every other
//! localhost session (the app, a `localhost up`) through their Run records:
//! a Service is running when a recorded Run's process tree is still alive.
//! That is all another process can know; Starting, Exited or Failed are
//! states of the session that runs the Service.

use std::path::Path;
use std::time::SystemTime;

use localhost_core::{Orphan, ProjectLibrary, RunRegistry, default_storage_root};

use crate::resolve;
use crate::terminal::{Terminal, abbreviated, duration};

pub fn run(library: &ProjectLibrary, query: &str, terminal: &Terminal) -> Result<(), String> {
    let project = resolve::project(&library.projects(), query)?;
    terminal.print(&format!(
        "{} {}",
        terminal.bold(&project.name),
        terminal.dim(&format!("({}) · {}", project.key, abbreviated(&project.path)))
    ));
    let runs = live_runs(&default_storage_root());
    let header = ["SERVICE", "NAME", "STATE", "PORT", "PID", "UP"].map(String::from).to_vec();
    let rows = project.services.iter().map(|service| {
        let port = service.port.map(|port| port.to_string()).unwrap_or_default();
        let mut row = vec![service.key.to_string(), service.name.clone()];
        match runs.iter().find(|run| run.service_id == service.id) {
            Some(run) => {
                let up = SystemTime::now().duration_since(run.started_at).unwrap_or_default();
                row.extend(["running".into(), port, run.process_group_id.to_string(), duration(up)]);
            }
            None => row.extend(["not running".into(), port]),
        }
        row
    });
    terminal.table(&std::iter::once(header).chain(rows).collect::<Vec<_>>());
    Ok(())
}

/// The live Runs of other localhost sessions. To a separate process every
/// one of them looks like an Orphan: the Run records can't tell a session
/// that crashed from one that is still running.
pub fn live_runs(storage_root: &Path) -> Vec<Orphan> {
    RunRegistry::new(storage_root).orphans()
}
