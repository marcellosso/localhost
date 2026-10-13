//! `localhost projects`: every Project, in the app's sidebar order.

use localhost_core::ProjectLibrary;

use crate::terminal::{Terminal, abbreviated};

pub fn run(library: &ProjectLibrary, terminal: &Terminal) {
    let projects = library.projects();
    if projects.is_empty() {
        terminal.error("No Projects yet. Add one in the localhost app.");
        return;
    }
    let header = ["KEY", "NAME", "SERVICES", "PATH"].map(String::from).to_vec();
    let rows = projects.iter().map(|project| {
        vec![
            project.key.to_string(),
            project.name.clone(),
            project.services.len().to_string(),
            abbreviated(&project.path),
        ]
    });
    terminal.table(&std::iter::once(header).chain(rows).collect::<Vec<_>>());
}
