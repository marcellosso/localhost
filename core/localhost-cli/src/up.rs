//! `localhost up <project> [service…]`: runs Services in the foreground,
//! printing their output, until Ctrl-C (or SIGTERM, or the terminal closing)
//! stops them all, or until every one has ended on its own.
//!
//! The Services run in this process's own ServiceManager, so they start in
//! dependency order, move off a taken Port, and stop SIGINT → SIGTERM →
//! SIGKILL exactly as they do in the app.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use localhost_core::{
    Id, LogStore, Origin, Project, ProjectLibrary, RunRegistry, ServiceEvent, ServiceManager, ServiceManagerOptions,
    ServiceOutputLine, ServiceState, ServiceStatus, default_storage_root,
};
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinSet;

use crate::logs::prefix_width;
use crate::resolve;
use crate::terminal::Terminal;

/// How long to wait, once Runs have ended, for the last of their output.
/// Only a process that left the Run's process group can hold it up.
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

pub fn run(library: ProjectLibrary, query: &str, queries: &[String], terminal: &Terminal) -> Result<ExitCode, String> {
    let project = resolve::project(&library.projects(), query)?;
    let mut requested: Vec<Id> = Vec::new();
    for query in queries {
        let service = resolve::service(&project, query)?;
        if !requested.contains(&service.id) {
            requested.push(service.id);
        }
    }
    let starting = if requested.is_empty() {
        let included: Vec<Id> =
            project.services.iter().filter(|service| !service.is_excluded_from_start_all).map(|s| s.id).collect();
        with_dependencies(&project, included)
    } else {
        with_dependencies(&project, requested.clone())
    };
    if starting.is_empty() {
        return Err(format!("{} has no Services to start.", project.name));
    }

    let storage_root = default_storage_root();
    let registry = Arc::new(RunRegistry::new(&storage_root));
    // Running a Service twice would only fight over its Port and its files.
    if let Some(run) = crate::status::live_runs(&storage_root).iter().find(|run| starting.contains(&run.service_id)) {
        let key = project.service(run.service_id).map(|service| service.key.to_string()).unwrap_or_default();
        return Err(format!(
            "{key} is already running (pid {}) in another localhost session. Stop it there first.",
            run.process_group_id
        ));
    }

    let names: Vec<String> =
        starting.iter().filter_map(|id| project.service(*id)).map(|service| service.key.to_string()).collect();
    terminal.error(&format!("Starting {}: {}. Ctrl-C stops them.", project.name, names.join(", ")));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Couldn't start: {error}"))?;
    let logs = Arc::new(LogStore::new(&storage_root));
    runtime.block_on(async move {
        let manager = ServiceManager::new(Arc::new(library), logs.clone(), registry, ServiceManagerOptions::default());
        let mut session = Session::new(terminal, &project, &manager, &logs);
        let mut signals = Signals::new().map_err(|error| format!("Couldn't watch for Ctrl-C: {error}"))?;

        let start = {
            let manager = manager.clone();
            let project_id = project.id;
            tokio::spawn(async move {
                if requested.is_empty() {
                    let _ = manager.start_all(project_id, Origin::Cli).await;
                    return;
                }
                let mut starts = JoinSet::new();
                for service_id in requested {
                    let manager = manager.clone();
                    starts.spawn(async move { manager.start(service_id, None, Origin::Cli).await });
                }
                starts.join_all().await;
            })
        };
        let mut start = Some(start);

        let ended_on_its_own = session
            .while_printing(async {
                tokio::select! {
                    _ = signals.next() => false,
                    _ = async {
                        let _ = start.as_mut().unwrap().await;
                        start = None;
                        wait_until_nothing_is_active(&manager, &project).await;
                    } => true,
                }
            })
            .await;

        if !ended_on_its_own {
            terminal.error("Stopping…");
            session
                .while_printing(async {
                    manager.stop_every_run(Origin::Cli).await;
                    // A start that was launching when the stop began may
                    // have launched since; stop that one too.
                    if let Some(start) = start {
                        let _ = start.await;
                        manager.stop_every_run(Origin::Cli).await;
                    }
                })
                .await;
        }
        session.finish().await;
        Ok(if session.has_failure { ExitCode::FAILURE } else { ExitCode::SUCCESS })
    })
}

/// `service_ids` and every Service they depend on, directly or not, in
/// dependency order.
pub(crate) fn with_dependencies(project: &Project, mut service_ids: Vec<Id>) -> Vec<Id> {
    let mut index = 0;
    while index < service_ids.len() {
        if let Some(service) = project.service(service_ids[index]) {
            for dependency in &service.dependencies {
                if !service_ids.contains(dependency) {
                    service_ids.push(*dependency);
                }
            }
        }
        index += 1;
    }
    project.dependency_order(&service_ids)
}

async fn wait_until_nothing_is_active(manager: &ServiceManager, project: &Project) {
    while project.services.iter().any(|service| manager.status(service.id).state.is_active()) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// SIGINT (Ctrl-C), SIGTERM, and SIGHUP (the terminal closed). Once these
/// are watched, none of them ends the process: it stops its Services first.
struct Signals(Vec<Signal>);

impl Signals {
    fn new() -> std::io::Result<Signals> {
        let kinds = [SignalKind::interrupt(), SignalKind::terminate(), SignalKind::hangup()];
        Ok(Signals(kinds.into_iter().map(signal).collect::<Result<_, _>>()?))
    }

    async fn next(&mut self) {
        let [interrupt, terminate, hang_up] = &mut self.0[..] else { unreachable!() };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
            _ = hang_up.recv() => {}
        }
    }
}

/// Prints the Project's output and state changes while `up` runs.
struct Session<'a> {
    terminal: &'a Terminal,
    project: &'a Project,
    logs: &'a LogStore,
    width: usize,
    output: UnboundedReceiver<ServiceOutputLine>,
    events: UnboundedReceiver<ServiceEvent>,
    /// The last note about each Service, so none is printed twice in a row.
    notes: HashMap<Id, Option<String>>,
    runs: Vec<Id>,
    has_failure: bool,
}

impl<'a> Session<'a> {
    fn new(terminal: &'a Terminal, project: &'a Project, manager: &ServiceManager, logs: &'a LogStore) -> Session<'a> {
        let service_ids: HashSet<Id> = project.services.iter().map(|service| service.id).collect();
        let (_, output) = logs.subscribe_to_services(service_ids);
        let (_, events) = manager.subscribe();
        Session {
            terminal,
            project,
            logs,
            width: prefix_width(&project.services),
            output,
            events,
            notes: HashMap::new(),
            runs: Vec::new(),
            has_failure: false,
        }
    }

    /// Awaits `future`, printing output and state changes meanwhile.
    async fn while_printing<F: Future>(&mut self, future: F) -> F::Output {
        tokio::pin!(future);
        loop {
            tokio::select! {
                value = &mut future => return value,
                Some(line) = self.output.recv() => self.print_output(&line),
                Some(event) = self.events.recv() => self.report(event),
            }
        }
    }

    /// Prints what is left once every Run has ended. A Run's last lines (how
    /// it ended) are written after its end is announced, so this waits for
    /// each Run's output to end.
    async fn finish(&mut self) {
        let runs: Vec<Id> = self.runs.clone();
        let logs = self.logs;
        let ended = async {
            for run_id in runs {
                let (_, mut stream) = logs.subscribe_to_run(run_id);
                while stream.recv().await.is_some() {}
            }
        };
        let _ = tokio::time::timeout(OUTPUT_DRAIN_TIMEOUT, self.while_printing(ended)).await;
        while let Ok(line) = self.output.try_recv() {
            self.print_output(&line);
        }
        while let Ok(event) = self.events.try_recv() {
            self.report(event);
        }
    }

    fn prefix(&self, service_id: Id, separator: &str) -> String {
        let key = self.project.service(service_id).map(|service| service.key.as_str()).unwrap_or("?");
        format!("{key:<width$} {separator}", width = self.width)
    }

    fn print_output(&self, line: &ServiceOutputLine) {
        let text = self.terminal.output(&line.line.text);
        let text =
            if line.line.channel == localhost_core::OutputChannel::Localhost { self.terminal.dim(&text) } else { text };
        self.terminal.print(&format!("{} {text}", self.terminal.dim(&self.prefix(line.service_id, "|"))));
    }

    fn report(&mut self, event: ServiceEvent) {
        match event {
            ServiceEvent::RunStarted(run) => self.runs.push(run.id),
            ServiceEvent::RunEnded(_) => {}
            ServiceEvent::ServiceStateChanged(status) => {
                // The same state is announced again when its details change
                // (the Dependencies still awaited, a restart becoming needed).
                let note = self.note(&status);
                if self.notes.insert(status.service_id, note.clone()) == Some(note.clone()) {
                    return;
                }
                if let Some(note) = note {
                    let prefix = self.terminal.bold(&self.prefix(status.service_id, "▸"));
                    self.terminal.print(&format!("{prefix} {note}"));
                }
            }
        }
    }

    /// What to say about a Service entering its state, if anything. Exits
    /// need nothing: the Run's log already says how it ended.
    fn note(&mut self, status: &ServiceStatus) -> Option<String> {
        let service = self.project.service(status.service_id)?;
        match status.state {
            ServiceState::Starting if !status.awaited_dependencies.is_empty() => {
                let keys: Vec<String> = status
                    .awaited_dependencies
                    .iter()
                    .filter_map(|id| self.project.service(*id))
                    .map(|dependency| dependency.key.to_string())
                    .collect();
                Some(format!("waiting for {}", keys.join(", ")))
            }
            ServiceState::Running => {
                let run = status.run.as_ref();
                let moved = run
                    .and_then(|run| Some((run.port?, run.port_change.as_ref()?)))
                    .map(|(port, change)| format!(" on port {port} instead of {}", change.configured_port))
                    .unwrap_or_default();
                let url = service.url_to_open(run).map(|url| format!(" · {url}")).unwrap_or_default();
                Some(format!("running{moved}{url}"))
            }
            ServiceState::Failed => {
                self.has_failure = true;
                let run = status.run.as_ref();
                let reason = run
                    .and_then(|run| run.failure.as_ref().map(|failure| failure.message()))
                    .or_else(|| run.and_then(|run| run.termination).map(|termination| termination.summary()))
                    .unwrap_or_else(|| "it ended unexpectedly.".into());
                Some(format!("failed: {reason}"))
            }
            _ => None,
        }
    }
}
