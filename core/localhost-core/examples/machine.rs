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
//! prints its URL. `quit` (or Ctrl-D) ends the connection and its Forwards.
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
    let machine = machines.add(None, destination)?;
    let (_, _, mut machine_events) = machines.subscribe();
    machines.connect(machine.id)?;
    println!("Connecting to {}…", machine.name);
    loop {
        match machine_events.recv().await {
            Some(MachinesEvent::StateChanged { state: MachineState::Connected { version }, .. }) => {
                println!("Connected: localhost {version} there.");
                break;
            }
            Some(MachinesEvent::StateChanged { state: MachineState::Unavailable { problem, is_retrying }, .. }) => {
                println!("{}", problem.message(&machine.name));
                if !is_retrying {
                    return Ok(());
                }
            }
            Some(_) => {}
            None => return Ok(()),
        }
    }

    let connection = machines.connection(machine.id)?;
    let (mut projects, mut project_updates) = connection.projects().await?;
    let (mut statuses, mut service_events) = connection.subscribe().await?;
    print_projects(&projects, &statuses);
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
                let Some((project, service)) = find(&projects, target) else {
                    println!("No Service {target} there.");
                    continue;
                };
                let result = match verb {
                    "start" => connection.start(service, None).await.map(|_| ()),
                    "stop" => connection.stop(service).await.map(|_| ()),
                    "restart" => connection.restart(service).await.map(|_| ()),
                    "open" => open(&machines, machine.id, &projects, &statuses, project, service).await,
                    _ => { println!("start|stop|restart|open <project>/<service>, or quit"); continue }
                };
                if let Err(error) = result {
                    println!("{}", error.message());
                }
            }
            Some(updated) = project_updates.recv() => {
                projects = updated;
                println!("The Projects there changed.");
                print_projects(&projects, &statuses);
            }
            Some(event) = service_events.recv() => {
                if let ServiceEvent::ServiceStateChanged(status) = &event {
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
                    statuses.insert(status.service_id, status.clone());
                }
            }
            Some(MachinesEvent::StateChanged { state, .. }) = machine_events.recv() => {
                match state {
                    MachineState::Unavailable { problem, is_retrying } => {
                        println!("{}{}", problem.message(&machine.name), if is_retrying { " Trying again…" } else { "" });
                    }
                    MachineState::Connected { .. } => {
                        // A new connection: its Projects and statuses afresh.
                        println!("Connected again.");
                        let connection = machines.connection(machine.id)?;
                        (projects, project_updates) = connection.projects().await?;
                        (statuses, service_events) = connection.subscribe().await?;
                        print_projects(&projects, &statuses);
                    }
                    _ => {}
                }
            }
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

/// Forwards the Service's Port, the same number here, and prints its URL.
async fn open(
    machines: &Machines,
    machine_id: Id,
    projects: &[Project],
    statuses: &HashMap<Id, ServiceStatus>,
    project_id: Id,
    service_id: Id,
) -> Result<(), LocalhostError> {
    let service = projects
        .iter()
        .find(|project| project.id == project_id)
        .and_then(|project| project.services.iter().find(|service| service.id == service_id))
        .expect("found above");
    let run = statuses.get(&service_id).and_then(|status| status.run.as_ref());
    let Some(port) = run.and_then(|run| run.port).or(service.port) else {
        println!("{} has no Port.", service.key);
        return Ok(());
    };
    let forward = machines.forward(machine_id, port, port).await?;
    println!("Forwarded {} there to {} here.", forward.port, forward.local_port);
    if let Some(url) = service.url_to_open(run) {
        println!("Open {url}");
    }
    Ok(())
}

fn find(projects: &[Project], target: &str) -> Option<(Id, Id)> {
    let (project_key, service_key) = target.split_once('/')?;
    let project = projects.iter().find(|project| project.key.to_string() == project_key)?;
    let service = project.services.iter().find(|service| service.key.to_string() == service_key)?;
    Some((project.id, service.id))
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
