//! `localhost`: the core without a UI. It reads the same storage as the
//! app, so it lists the same Projects, runs Services in the foreground, and
//! serves them to coding agents over MCP, through the core it shares with
//! the app (`localhost daemon`).

mod arguments;
mod logs;
mod mcp;
mod projects;
mod resolve;
mod signals;
mod status;
mod terminal;
mod up;

use std::process::ExitCode;
use std::sync::Arc;

use localhost_core::{ProjectLibrary, SystemSecretStore, default_storage_root};

use crate::arguments::Command;
use crate::terminal::Terminal;

fn main() -> ExitCode {
    let terminal = Terminal::new();
    let command = match arguments::parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(problem) => {
            terminal.error(&format!("{problem}\nRun `localhost --help` for usage."));
            return ExitCode::from(2);
        }
    };
    match run(command, &terminal) {
        Ok(code) => code,
        Err(message) => {
            terminal.error(&message);
            ExitCode::FAILURE
        }
    }
}

/// Runs a command; `Err` is the message to print before failing.
fn run(command: Command, terminal: &Terminal) -> Result<ExitCode, String> {
    let open = || -> Result<ProjectLibrary, String> {
        let library = ProjectLibrary::open(&default_storage_root(), Arc::new(SystemSecretStore::new()))
            .map_err(|error| error.message())?;
        for file in library.unreadable_files() {
            let moved = file.moved_to.as_ref().map(|path| format!(" It was moved to {path}.")).unwrap_or_default();
            terminal.error(&format!("Couldn't read {}: {}{moved}", file.path, file.error.message()));
        }
        Ok(library)
    };
    match command {
        Command::Help => terminal.print(arguments::USAGE),
        Command::Version => terminal.print(&format!("localhost {}", localhost_core::VERSION)),
        Command::Projects => projects::run(&open()?, terminal),
        Command::Status { project } => status::run(&open()?, &project, terminal)?,
        Command::Logs { project, service } => logs::run(&open()?, &project, service.as_deref(), terminal)?,
        Command::Up { project, services } => return up::run(open()?, &project, &services, terminal),
        Command::Mcp { read_only } => return mcp::run(open()?, read_only),
        Command::Daemon { storage_root } => localhost_core::remote::shared::run(storage_root.as_ref())?,
    }
    Ok(ExitCode::SUCCESS)
}
