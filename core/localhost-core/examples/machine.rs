//! Views another Machine from a terminal, through the same `Machines` the
//! apps use, to try a Machine before any app shows one:
//!
//! ```sh
//! cargo run -p localhost-core --example machine -- devbox
//! ```
//!
//! It lists the other Machine's Projects and Services and follows their
//! changes. Type `start`, `stop`, `restart` or `open` and a
//! `<project>/<service>` Key pair; `open` forwards the Service's Port and
//! prints its URL, as Open in Browser does. `quit` (or Ctrl-D) ends the connection and its Forwards.
//! The Machine list is kept in a temporary folder, never in localhost's own.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use localhost_core::machine::{MachineState, Machines, MachinesEvent};
use localhost_core::{Id, LocalhostError, Project, ServiceEvent, ServiceState, ServiceStatus};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let Some(destination) = std::env::args().nth(1) else {
        eprintln!("Usage: cargo run -p localhost-core --example machine -- <ssh destination>");
        std::process::exit(2);
    };
    if let Err(error) = run(&destination).await {
        eprintln!("{}", error.message());
        std::process::exit(1);
    }
}

async fn run(destination: &str) -> Result<(), LocalhostError> {
    let storage_root = std::env::temp_dir().join(format!("localhost-machine-example-{}", std::process::id()));
    let machines = Machines::open(&storage_root, Path::new("ssh"))?;
    let (_, mut machine_events) = machines.subscribe();
    println!("Connecting to {destination}…");
    // Added only once it is reached; then kept connected.
    let machine = machines.add(None, destination).await?;
    let mut projects = Vec::new();
    let mut statuses = HashMap::new();
    println!("\nstart|stop|restart|open <project>/<service>, or quit");

    let mut input = lines_typed();
    loop {
        tokio::select! {
            line = input.recv() => {
                let Some(line) = line else { break };
                let words: Vec<&str> = line.split_whitespace().collect();
                let (verb, target) = match words.as_slice() {
                    ["quit"] => break,
                    [verb, target] => (*verb, *target),
                    [] => continue,
                    _ => { println!("start|stop|restart|open <project>/<service>, or quit"); continue }
                };
                let Some(service) = find(&projects, target) else {
                    println!("No Service {target} there.");
                    continue;
                };
                let result = match verb {
                    "start" => machines.start(machine.id, service, None).await.map(|_| ()),
                    "stop" => machines.stop(machine.id, service).await.map(|_| ()),
                    "restart" => machines.restart(machine.id, service).await.map(|_| ()),
                    "open" => match machines.open_service(machine.id, service).await {
                        Ok(Some(url)) => { println!("Open {url}"); Ok(()) }
                        Ok(None) => { println!("{target} has nothing to open."); Ok(()) }
                        Err(error) => Err(error),
                    },
                    _ => { println!("start|stop|restart|open <project>/<service>, or quit"); continue }
                };
                if let Err(error) = result {
                    println!("{}", error.message());
                }
            }
            Some(event) = machine_events.recv() => match event {
                MachinesEvent::StateChanged { state, .. } => match state {
                    MachineState::Connected { version } => println!("Connected: localhost {version} there."),
                    MachineState::Unavailable { problem, is_retrying } => {
                        println!("{}{}", problem.message(&machine.name), if is_retrying { " Trying again…" } else { "" });
                    }
                    _ => {}
                },
                MachinesEvent::ProjectsChanged { projects: updated, .. } => {
                    projects = updated;
                    print_projects(&projects, &statuses);
                }
                MachinesEvent::StatusesChanged { statuses: updated, .. } => {
                    statuses = updated.into_iter().map(|status| (status.service_id, status)).collect();
                    print_projects(&projects, &statuses);
                }
                MachinesEvent::ServiceChanged { event: ServiceEvent::ServiceStateChanged(status), .. } => {
                    let run = status.run.as_ref();
                    println!(
                        "{} → {:?}{}{}",
                        name(&projects, status.service_id),
                        status.state,
                        run.map(|run| format!(", started by {:?}", run.origin)).unwrap_or_default(),
                        run.and_then(|run| run.stop_origin).map(|origin| format!(", stopped by {origin:?}")).unwrap_or_default(),
                    );
                    if let Some(failure) = run.and_then(|run| run.failure.as_ref()) {
                        println!("  {}", failure.message());
                    }
                    statuses.insert(status.service_id, status);
                }
                _ => {}
            },
        }
    }
    machines.disconnect(machine.id);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&storage_root);
    Ok(())
}

/// What's typed, line by line, read on a thread of its own.
fn lines_typed() -> mpsc::UnboundedReceiver<String> {
    let (sender, receiver) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

fn find(projects: &[Project], target: &str) -> Option<Id> {
    let (project_key, service_key) = target.split_once('/')?;
    let project = projects.iter().find(|project| project.key.to_string() == project_key)?;
    let service = project.services.iter().find(|service| service.key.to_string() == service_key)?;
    Some(service.id)
}

fn name(projects: &[Project], service_id: Id) -> String {
    projects
        .iter()
        .find_map(|project| {
            let service = project.services.iter().find(|service| service.id == service_id)?;
            Some(format!("{}/{}", project.key, service.key))
        })
        .unwrap_or_else(|| service_id.to_string())
}

fn print_projects(projects: &[Project], statuses: &HashMap<Id, ServiceStatus>) {
    for project in projects {
        println!("\n{} ({})", project.name, project.key);
        for service in &project.services {
            let state = statuses.get(&service.id).map_or(ServiceState::Stopped, |status| status.state);
            let port = service.port.map(|port| format!("  :{port}")).unwrap_or_default();
            println!("  {}/{}  {state:?}{port}", project.key, service.key);
        }
    }
}
