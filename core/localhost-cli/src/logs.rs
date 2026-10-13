//! `localhost logs <project> [service]`: the output of each Service's latest
//! Run, from its log file, whichever session ran it. One Service's lines are
//! printed as they are; a whole Project's are merged by time, each after its
//! Service's Key.

use localhost_core::{LogStore, OutputChannel, OutputLine, ProjectLibrary, Service, default_storage_root};

use crate::resolve;
use crate::terminal::Terminal;

pub fn run(library: &ProjectLibrary, project: &str, service: Option<&str>, terminal: &Terminal) -> Result<(), String> {
    let project = resolve::project(&library.projects(), project)?;
    let store = LogStore::new(&default_storage_root());
    let line = |service: Option<&Service>, line: &OutputLine, width: usize| {
        let text = terminal.output(&line.text);
        let text = if line.channel == OutputChannel::Localhost { terminal.dim(&text) } else { text };
        match service {
            Some(service) => format!("{} {text}", terminal.dim(&format!("{:<width$} |", service.key.as_str()))),
            None => text,
        }
    };

    if let Some(query) = service {
        let service = resolve::service(&project, query)?;
        let lines = store.stored_lines(service.id);
        if lines.is_empty() {
            terminal.error(&format!("No logs yet for {}.", service.name));
        }
        for output in &lines {
            terminal.print(&line(None, output, 0));
        }
        return Ok(());
    }

    let mut lines: Vec<(&Service, OutputLine)> = project
        .services
        .iter()
        .flat_map(|service| store.stored_lines(service.id).into_iter().map(move |line| (service, line)))
        .collect();
    if lines.is_empty() {
        terminal.error(&format!("No logs yet for {}.", project.name));
        return Ok(());
    }
    // Stable, so each Service's lines keep their order.
    lines.sort_by_key(|(_, line)| line.timestamp);
    let width = prefix_width(lines.iter().map(|(service, _)| *service));
    for (service, output) in &lines {
        terminal.print(&line(Some(service), output, width));
    }
    Ok(())
}

/// The width of the widest Key among `services`, to align their lines.
pub fn prefix_width<'a>(services: impl IntoIterator<Item = &'a Service>) -> usize {
    services.into_iter().map(|service| service.key.as_str().chars().count()).max().unwrap_or(0)
}
