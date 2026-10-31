//! `localhost mcp`, run as a real process against a temporary home folder and
//! driven over its standard input and output as a coding agent's client would.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use localhost_core::remote::{Client as CoreClient, shared};
use localhost_core::{
    ExecutionTarget, Id, Key, Link, MemorySecretStore, NewService, Origin, Project, ProjectLibrary, Service,
    ServiceManagerOptions, ServiceState, Targets,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// How long a test waits for something before failing instead of hanging.
const PATIENCE: Duration = Duration::from_secs(60);

struct Home {
    directory: TempDir,
}

impl Home {
    fn new() -> Home {
        Home { directory: tempfile::tempdir().unwrap() }
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn storage_root(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.path().join("Library/Application Support/localhost")
        } else if cfg!(windows) {
            self.path().join("AppData").join("Roaming").join("localhost")
        } else {
            self.path().join("data/localhost")
        }
    }

    fn library(&self) -> ProjectLibrary {
        ProjectLibrary::open(&self.storage_root(), Arc::new(MemorySecretStore::new())).unwrap()
    }

    fn project(&self, name: &str, key: &str) -> Project {
        let folder = self.path().join("code").join(key);
        std::fs::create_dir_all(&folder).unwrap();
        self.library()
            .add_project(name, Key::new(key).unwrap(), folder.to_str().unwrap(), vec![], Origin::MainWindow)
            .unwrap()
    }

    fn service(
        &self,
        project: &Project,
        key: &str,
        command_line: &str,
        port: Option<u16>,
        dependencies: Vec<Id>,
    ) -> Service {
        let mut service = NewService::new(key.to_uppercase(), Key::new(key).unwrap(), command_line);
        service.port = port.map(i64::from);
        self.library().add_service(project.id, service, vec![], dependencies, Origin::MainWindow).unwrap()
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_localhost"));
        command
            .args(arguments)
            .current_dir(self.path())
            .env("HOME", self.path())
            .env("XDG_DATA_HOME", self.path().join("data"))
            .env("APPDATA", self.path().join("AppData").join("Roaming"))
            .env("SHELL", "/bin/sh");
        command
    }

    fn mcp(&self) -> Client {
        self.mcp_with(&["mcp"])
    }

    fn mcp_with(&self, arguments: &[&str]) -> Client {
        let mut child = self
            .command(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let message: Value = serde_json::from_str(&line).expect("stdout carries JSON-RPC only");
                if sender.send(message).is_err() {
                    return;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut client = Client { child, stdin, messages, next_id: 0 };
        let reply = client.request("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }));
        assert_eq!(reply["result"]["serverInfo"]["name"], "localhost");
        client.notify("notifications/initialized");
        client
    }

    /// The app's side of the shared core, as the Linux and Windows apps reach it.
    /// Connect an agent first: the core it starts runs with this home's
    /// environment, which the test process doesn't have.
    async fn app(&self) -> Targets {
        let root = self.storage_root();
        let library = Arc::new(self.library());
        let launcher = Box::new(move |target: &ExecutionTarget| match target {
            ExecutionTarget::ThisMachine => {
                let mut daemon = Command::new(env!("CARGO_BIN_EXE_localhost"));
                daemon.arg("daemon").arg(&root);
                shared::link(&root, CoreClient::App, daemon)
            }
            ExecutionTarget::Wsl { .. } => Link::Unavailable("not here".into()),
        });
        let targets = Targets::open(library, &self.storage_root(), ServiceManagerOptions::default(), launcher);
        targets.connect(&ExecutionTarget::ThisMachine).await.unwrap();
        targets
    }

    /// Waits for the file a Service writes when it has started: its shell's
    /// pid, which is its process group.
    fn wait_for_pid(&self, name: &str) -> i32 {
        let path = self.path().join(name);
        wait_until(|| std::fs::read_to_string(&path).is_ok_and(|text| text.ends_with('\n')), name);
        std::fs::read_to_string(&path).unwrap().trim().parse().unwrap()
    }
}

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Value>,
    next_id: u64,
}

impl Client {
    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let message = self.messages.recv_timeout(PATIENCE).expect("a reply");
            if message["id"] == id {
                return message;
            }
        }
    }

    /// A tool's result, which must not be an error.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call_result(tool, arguments);
        assert_eq!(result["isError"], false, "{tool}: {result:#}");
        result["structuredContent"].clone()
    }

    /// A tool's result, which must be an error: its `{ kind, message }`.
    fn call_error(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call_result(tool, arguments);
        assert_eq!(result["isError"], true, "{tool}: {result:#}");
        result["structuredContent"]["error"].clone()
    }

    fn call_result(&mut self, tool: &str, arguments: Value) -> Value {
        let reply = self.request("tools/call", json!({ "name": tool, "arguments": arguments }));
        reply["result"].clone()
    }

    /// Closes standard input, as a client disconnecting does, and waits for exit.
    fn disconnect(mut self) -> i32 {
        drop(self.stdin.take());
        self.wait_for_exit("its input closed")
    }

    /// Its exit code, once it has ended on its own.
    fn wait_for_exit(&mut self, after: &str) -> i32 {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("`localhost mcp` did not exit after {after}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn wait_until(condition: impl Fn() -> bool, what: &str) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn process_group_exists(group: i32) -> bool {
    unsafe { libc::kill(-group, 0) == 0 }
}

/// Whether the shell, whose pid names the Run's process group, is alive:
/// stopping a Run ends its whole job.
#[cfg(windows)]
fn process_group_exists(group: i32) -> bool {
    let filter = format!("PID eq {group}");
    let output = Command::new("tasklist").args(["/FI", &filter, "/FO", "CSV", "/NH"]).output().unwrap();
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{group}\""))
}

/// A command line that records its shell's pid in `name`, then stays up
/// with a grandchild, as most dev servers do.
#[cfg(unix)]
fn long_running(home: &Home, name: &str) -> String {
    format!("echo started {name}; sleep 1000 & echo $$ > '{}'; wait", home.path().join(name).display())
}

/// The same with Python, by its full path: the core's Runs start from the
/// user's environment, not the test runner's.
#[cfg(windows)]
fn long_running(home: &Home, name: &str) -> String {
    let script = home.path().join("long_running.py");
    std::fs::write(
        &script,
        "import os, sys, time\n\
         print('started', sys.argv[1], flush=True)\n\
         with open(sys.argv[2], 'w') as pid:\n    pid.write(f'{os.getppid()}\\n')\n\
         time.sleep(1000)\n",
    )
    .unwrap();
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|folder| folder.join("python.exe"))
        .find(|path| path.is_file())
        .expect("python on PATH");
    format!(r#""{}" "{}" {name} "{}""#, python.display(), script.display(), home.path().join(name).display())
}

fn service<'a>(result: &'a Value, key: &str) -> &'a Value {
    let services = result["services"].as_array().unwrap();
    services.iter().find(|service| service["key"] == key).unwrap_or_else(|| panic!("{key} in {result:#}"))
}

#[test]
fn lists_the_tools_and_answers_ping_and_unknown_methods() {
    let home = Home::new();
    let mut client = home.mcp();
    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    for name in [
        "list_projects",
        "list_services",
        "status",
        "start_service",
        "stop_service",
        "start_project",
        "stop_project",
        "logs",
        "ports",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    assert_eq!(client.request("resources/list", json!({}))["error"]["code"], -32601);
    assert_eq!(client.call("list_projects", json!({})), json!({ "projects": [] }));
    assert_eq!(client.disconnect(), 0);
}

#[test]
fn starts_a_service_with_its_dependencies_reads_its_logs_and_stops_it() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db"), None, vec![]);
    home.service(&project, "web", &long_running(&home, "web"), None, vec![db.id]);
    let mut client = home.mcp();

    let projects = client.call("list_projects", json!({}));
    assert_eq!(projects["projects"][0]["key"], "acme");
    assert_eq!(projects["projects"][0]["services"], json!(["db", "web"]));
    let services = client.call("list_services", json!({ "project": "acme" }));
    assert_eq!(service(&services, "web")["dependencies"], json!(["db"]));

    let started = client.call("start_service", json!({ "project": "acme", "service": "web" }));
    assert_eq!(service(&started, "web")["state"], "running", "{started:#}");
    assert_eq!(service(&started, "db")["state"], "running");
    let web_group = home.wait_for_pid("web");
    let db_group = home.wait_for_pid("db");

    let status = client.call("status", json!({ "project": "Acme Shop", "service": "web" }));
    assert_eq!(service(&status, "web")["pid"], web_group);
    assert_eq!(service(&status, "web")["session"], "shared");
    assert_eq!(service(&status, "web")["startedBy"], "agent");

    let logs = client.call("logs", json!({ "project": "acme", "service": "web" }));
    assert!(logs["lines"].as_array().unwrap().iter().any(|line| line["text"] == "started web"), "{logs:#}");
    let merged = client.call_result("logs", json!({ "project": "acme", "contains": "STARTED" }));
    let text = merged["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("db  | started db") && text.contains("web | started web"), "{text}");
    let none = client.call("logs", json!({ "project": "acme", "since_seconds": 1, "contains": "nothing like this" }));
    assert_eq!(none["matched"], 0);

    // Stopping a Dependency leaves its dependents alone.
    let stopped = client.call("stop_service", json!({ "project": "acme", "service": "web" }));
    assert_eq!(service(&stopped, "web")["state"], "stopped");
    assert!(!process_group_exists(web_group));
    assert!(process_group_exists(db_group));

    // Disconnecting stops everything this session started, with no app open.
    assert_eq!(client.disconnect(), 0);
    wait_until(|| !process_group_exists(db_group), "db to stop after the agent disconnected");
}

#[test]
fn start_and_stop_a_whole_project() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db"), None, vec![]);
    home.service(&project, "web", &long_running(&home, "web"), None, vec![db.id]);
    home.service(&project, "migrate", "echo migrated", None, vec![db.id]);
    let mut client = home.mcp();

    let started = client.call("start_project", json!({ "project": "acme" }));
    assert_eq!(service(&started, "db")["state"], "running", "{started:#}");
    assert_eq!(service(&started, "web")["state"], "running");
    assert_eq!(service(&started, "migrate")["state"], "exited");
    let groups = [home.wait_for_pid("db"), home.wait_for_pid("web")];

    let stopped = client.call("stop_project", json!({ "project": "acme" }));
    assert!(stopped["services"].as_array().unwrap().iter().all(|service| service["state"] != "running"), "{stopped:#}");
    assert!(groups.iter().all(|group| !process_group_exists(*group)));
    client.disconnect();
}

/// Shell syntax.
#[cfg(unix)]
#[test]
fn a_failed_start_says_why_with_its_last_output() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    home.service(&project, "web", "echo 'Error: cannot find module express' >&2; exit 3", None, vec![]);
    home.service(&project, "typo", "no-such-command-anywhere", None, vec![]);
    let mut client = home.mcp();

    let started = client.call("start_service", json!({ "project": "acme", "service": "web" }));
    let web = service(&started, "web");
    assert_eq!(web["state"], "failed", "{started:#}");
    assert_eq!(web["exit"], "Exited with code 3");
    assert!(
        web["lastOutput"].as_array().unwrap().iter().any(|line| line.as_str().unwrap().contains("cannot find module"))
    );

    let started = client.call("start_service", json!({ "project": "acme", "service": "typo" }));
    assert_eq!(service(&started, "typo")["failure"]["kind"], "commandNotFound", "{started:#}");
    client.disconnect();
}

#[test]
fn names_that_match_nothing_are_errors_that_list_the_keys() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    home.service(&project, "web", "true", None, vec![]);
    let mut client = home.mcp();

    let error = client.call_error("status", json!({ "project": "zzz" }));
    assert_eq!(error["kind"], "notFound");
    assert!(error["message"].as_str().unwrap().contains("acme"), "{error:#}");
    let error = client.call_error("start_service", json!({ "project": "acme", "service": "api" }));
    assert!(error["message"].as_str().unwrap().contains("web"), "{error:#}");
    assert_eq!(client.call_error("status", json!({}))["kind"], "invalidArguments");
    assert_eq!(client.call_error("logs", json!({ "project": "acme", "lines": 0 }))["kind"], "invalidArguments");
    client.disconnect();
}

#[test]
fn projects_added_after_connecting_are_seen() {
    let home = Home::new();
    let mut client = home.mcp();
    assert_eq!(client.call("list_projects", json!({}))["projects"], json!([]));
    home.project("Acme", "acme");
    assert_eq!(client.call("list_projects", json!({}))["projects"][0]["key"], "acme");
    client.disconnect();
}

/// Shell syntax, and SIGINT.
#[cfg(unix)]
#[test]
fn a_service_another_session_runs_is_never_started_twice_or_stopped_from_here() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db"), None, vec![]);
    home.service(&project, "web", &long_running(&home, "web"), None, vec![db.id]);
    let mut up = home
        .command(&["up", "acme", "db"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let db_group = home.wait_for_pid("db");
    let mut client = home.mcp();

    let status = client.call("status", json!({ "project": "acme", "service": "db" }));
    assert_eq!(service(&status, "db")["state"], "running");
    assert_eq!(service(&status, "db")["session"], "other");
    assert_eq!(service(&status, "db")["pid"], db_group);

    let started = client.call("start_service", json!({ "project": "acme", "service": "db" }));
    assert!(started["note"].as_str().unwrap().contains("another localhost session"), "{started:#}");
    assert_eq!(
        client.call_error("start_service", json!({ "project": "acme", "service": "web" }))["kind"],
        "runningInAnotherSession"
    );
    assert_eq!(
        client.call_error("stop_service", json!({ "project": "acme", "service": "db" }))["kind"],
        "runningInAnotherSession"
    );
    assert!(process_group_exists(db_group));

    client.disconnect();
    assert!(process_group_exists(db_group), "only the session's own Runs stop on disconnect");
    unsafe { libc::kill(up.id() as i32, libc::SIGINT) };
    up.wait().unwrap();
}

#[test]
fn ports_shows_who_holds_a_services_port() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    // Held by a process localhost didn't start, so it is Unmanaged.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut new_service = NewService::new("Web", Key::new("web").unwrap(), "true");
    new_service.port = Some(port.into());
    new_service.port_can_change = false;
    home.library().add_service(project.id, new_service, vec![], vec![], Origin::MainWindow).unwrap();
    let mut client = home.mcp();

    let ports = client.call("ports", json!({ "project": "acme" }));
    let held = ports["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|found| found["ports"].as_array().unwrap().contains(&json!(port)));
    let held = held.unwrap_or_else(|| panic!("port {port} in {ports:#}"));
    assert_eq!(held["holdsPortOf"], json!(["acme/web"]));
    assert_eq!(held["pid"], std::process::id());

    let started = client.call("start_service", json!({ "project": "acme", "service": "web" }));
    let web = service(&started, "web");
    assert_eq!(web["state"], "failed", "{started:#}");
    assert_eq!(web["failure"]["kind"], "portConflict");
    assert_eq!(web["failure"]["detail"]["port"], port);
    drop(listener);
    client.disconnect();
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()
}

fn wait_for_state(runtime: &tokio::runtime::Runtime, app: &Targets, service_id: Id, state: ServiceState) {
    runtime.block_on(async {
        let deadline = Instant::now() + PATIENCE;
        while app.status(service_id).state != state {
            assert!(Instant::now() < deadline, "timed out waiting for {state:?}: {:?}", app.status(service_id));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
}

#[test]
fn the_app_sees_what_an_agent_starts_and_the_reverse() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db"), None, vec![]);
    let web = home.service(&project, "web", &long_running(&home, "web"), None, vec![]);
    let mut agent = home.mcp();
    let runtime = runtime();
    let app = runtime.block_on(home.app());

    let started = agent.call("start_service", json!({ "project": "acme", "service": "web" }));
    assert_eq!(service(&started, "web")["state"], "running", "{started:#}");
    let web_group = home.wait_for_pid("web");
    wait_for_state(&runtime, &app, web.id, ServiceState::Running);
    assert_eq!(app.status(web.id).run.unwrap().origin, Origin::Agent);

    runtime.block_on(app.start(db.id, None, Origin::MainWindow)).unwrap();
    wait_for_state(&runtime, &app, db.id, ServiceState::Running);
    let db_group = home.wait_for_pid("db");
    let status = agent.call("status", json!({ "project": "acme", "service": "db" }));
    assert_eq!(service(&status, "db")["state"], "running", "{status:#}");
    assert_eq!(service(&status, "db")["session"], "shared");
    assert_eq!(service(&status, "db")["startedBy"], "app");
    assert_eq!(service(&status, "db")["pid"], db_group);

    // A stop from either stops the one Run.
    let stopped = agent.call("stop_service", json!({ "project": "acme", "service": "db" }));
    assert_eq!(service(&stopped, "db")["state"], "stopped", "{stopped:#}");
    assert!(!process_group_exists(db_group));
    wait_for_state(&runtime, &app, db.id, ServiceState::Stopped);

    runtime.block_on(app.stop(web.id, Origin::MainWindow)).unwrap();
    assert!(!process_group_exists(web_group));
    let status = agent.call("status", json!({ "project": "acme", "service": "web" }));
    assert_eq!(service(&status, "web")["state"], "stopped", "{status:#}");

    agent.disconnect();
    drop(app);
}

#[test]
fn the_app_counts_connected_agents_and_disconnects_them_before_an_update() {
    let home = Home::new();
    home.project("Acme", "acme");
    let mut first = home.mcp();
    let mut second = home.mcp();
    let runtime = runtime();
    let app = runtime.block_on(home.app());
    assert_eq!(runtime.block_on(app.connected_agent_count()), 2);

    assert_eq!(runtime.block_on(app.disconnect_agents()), 2);
    // Their input is still open: they end because the core dismissed them.
    assert_eq!(first.wait_for_exit("the app disconnected it"), 0);
    assert_eq!(second.wait_for_exit("the app disconnected it"), 0);
    assert_eq!(runtime.block_on(app.connected_agent_count()), 0);
    drop(app);
}

#[test]
fn an_agent_disconnecting_leaves_running_what_the_app_shows() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    let web = home.service(&project, "web", &long_running(&home, "web"), None, vec![]);
    let mut agent = home.mcp();
    let runtime = runtime();
    let app = runtime.block_on(home.app());

    agent.call("start_service", json!({ "project": "acme", "service": "web" }));
    let web_group = home.wait_for_pid("web");
    assert_eq!(agent.disconnect(), 0);
    std::thread::sleep(Duration::from_secs(1));
    assert!(process_group_exists(web_group), "the app is open, so the agent's Run goes on");
    assert_eq!(app.status(web.id).state, ServiceState::Running);

    // With no client left that needs it, it stops.
    drop(app);
    drop(runtime);
    wait_until(|| !process_group_exists(web_group), "web to stop once the app disconnected too");
}

#[test]
fn an_agent_disconnecting_leaves_running_what_another_agent_started() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    home.service(&project, "db", &long_running(&home, "db"), None, vec![]);
    home.service(&project, "web", &long_running(&home, "web"), None, vec![]);
    let mut first = home.mcp();
    let mut second = home.mcp();

    first.call("start_service", json!({ "project": "acme", "service": "web" }));
    second.call("start_service", json!({ "project": "acme", "service": "db" }));
    let (web_group, db_group) = (home.wait_for_pid("web"), home.wait_for_pid("db"));
    let status = second.call("status", json!({ "project": "acme", "service": "web" }));
    assert_eq!(service(&status, "web")["state"], "running", "{status:#}");

    first.disconnect();
    wait_until(|| !process_group_exists(web_group), "the first agent's Run to stop");
    assert!(process_group_exists(db_group), "the second agent's Run goes on");
    second.disconnect();
    wait_until(|| !process_group_exists(db_group), "the second agent's Run to stop");
}

// MARK: Changing the Project Configuration

/// Waits until the app's library has what `check` looks for, without the
/// app reading the Configuration itself: the core tells it.
fn wait_for_app(app: &Targets, check: impl Fn(&[Project]) -> bool, what: &str) {
    wait_until(|| check(&app.library().projects()), what);
}

#[test]
fn an_agent_adds_edits_and_removes_projects_and_services_and_the_open_app_shows_each_change() {
    let home = Home::new();
    let folder = home.path().join("code").join("shop");
    std::fs::create_dir_all(&folder).unwrap();
    let mut agent = home.mcp();
    let runtime = runtime();
    let app = runtime.block_on(home.app());

    let created =
        agent.call("create_project", json!({ "name": "Acme Shop", "path": folder, "env": { "MODE": "dev" } }));
    let project = &created["project"];
    assert_eq!(project["key"], "acme-shop", "{created:#}");
    assert_eq!(project["env"], json!({ "MODE": "dev" }));
    let project_id: Id = project["id"].as_str().unwrap().parse().unwrap();
    wait_for_app(&app, |projects| projects.iter().any(|project| project.id == project_id), "the new Project");

    let db = agent.call(
        "create_service",
        json!({ "project": "acme-shop", "name": "Postgres", "key": "db", "command_line": "docker compose up db", "port": 5432, "port_can_change": false }),
    );
    assert_eq!(db["service"]["key"], "db", "{db:#}");
    let web = agent.call(
        "create_service",
        json!({ "project": "acme-shop", "name": "Web", "command_line": "pnpm dev", "working_directory": "./apps/web/", "port": 3000, "dependencies": ["db"], "env": { "DEBUG": 1 } }),
    );
    let web = &web["service"];
    assert_eq!(web["workingDirectory"], "apps/web", "{web:#}");
    assert_eq!(web["dependencies"], json!(["db"]));
    assert_eq!(web["env"], json!({ "DEBUG": "1" }));
    let web_id: Id = web["id"].as_str().unwrap().parse().unwrap();
    wait_for_app(
        &app,
        |projects| projects.iter().any(|project| project.services.iter().any(|service| service.id == web_id)),
        "the new Services",
    );
    let (_, saved) = app.library().service(web_id).unwrap();
    assert_eq!(saved.command_line, "pnpm dev");
    assert_eq!(saved.port, Some(3000));
    assert!(!std::fs::read_dir(&folder).unwrap().any(|_| true), "nothing is written into the folder");

    let edited = agent.call(
        "update_service",
        json!({ "project": "acme-shop", "service": "web", "port": null, "command_line": "pnpm start" }),
    );
    assert_eq!(edited["service"]["port"], Value::Null, "{edited:#}");
    assert_eq!(edited["service"]["dependencies"], json!(["db"]), "fields left out stay");
    wait_for_app(
        &app,
        |_| app.library().service(web_id).is_ok_and(|(_, service)| service.command_line == "pnpm start"),
        "the edited Service",
    );
    let renamed = agent
        .call("update_project", json!({ "project": "acme-shop", "name": "Acme", "key": "acme", "favorite": true }));
    assert_eq!(renamed["project"]["key"], "acme", "{renamed:#}");
    assert_eq!(renamed["project"]["env"], json!({ "MODE": "dev" }));
    wait_for_app(
        &app,
        |projects| projects.iter().any(|project| project.key.as_str() == "acme" && project.is_favorite),
        "the edited Project",
    );

    let removed = agent.call("delete_service", json!({ "project": "acme", "service": "db", "confirm": true }));
    assert_eq!(removed["removed"]["key"], "db", "{removed:#}");
    assert_eq!(removed["note"], "No longer a Dependency of: web.");
    wait_for_app(&app, |projects| projects.iter().all(|project| project.services.len() == 1), "the removed Service");
    assert!(app.library().service(web_id).unwrap().1.dependencies.is_empty());

    let removed = agent.call("delete_project", json!({ "project": "acme", "confirm": true }));
    assert_eq!(removed["removed"]["services"], json!(["web"]), "{removed:#}");
    wait_for_app(&app, |projects| projects.is_empty(), "the removed Project");
    assert!(folder.is_dir(), "the folder is left as it was");
    assert!(home.library().projects().is_empty());

    agent.disconnect();
    drop(app);
}

#[test]
fn removing_needs_confirmation_and_stops_what_runs() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    home.service(&project, "web", &long_running(&home, "web"), None, vec![]);
    let mut agent = home.mcp();

    let error = agent.call_error("delete_service", json!({ "project": "acme", "service": "web" }));
    assert_eq!(error["kind"], "confirmationRequired");
    assert!(error["message"].as_str().unwrap().contains("confirm: true"), "{error:#}");
    let error = agent.call_error("delete_project", json!({ "project": "acme", "confirm": false }));
    assert_eq!(error["kind"], "confirmationRequired");
    assert_eq!(home.library().projects()[0].services.len(), 1, "nothing was removed");

    agent.call("start_service", json!({ "project": "acme", "service": "web" }));
    let group = home.wait_for_pid("web");
    let removed = agent.call("delete_service", json!({ "project": "acme", "service": "web", "confirm": true }));
    assert_eq!(removed["stopped"], json!(["web"]), "{removed:#}");
    assert!(!process_group_exists(group));
    assert!(home.library().projects()[0].services.is_empty());
    agent.disconnect();
}

#[test]
fn changes_are_checked_as_the_apps_editors_check_them() {
    let home = Home::new();
    let project = home.project("Acme", "acme");
    home.service(&project, "web", "pnpm dev", None, vec![]);
    let mut agent = home.mcp();
    let folder = home.path().join("code").join("acme");

    let missing = home.path().join("missing");
    let error = agent.call_error("create_project", json!({ "name": "Gone", "path": missing }));
    assert_eq!(error["kind"], "projectFolderMissing", "{error:#}");
    let error = agent.call_error("create_project", json!({ "name": "Relative", "path": "code/acme" }));
    assert_eq!(error["kind"], "invalidProjectPath");
    let error = agent.call_error("create_project", json!({ "name": "Again", "key": "acme", "path": folder }));
    assert_eq!(error["kind"], "duplicateProjectKey");
    let error =
        agent.call_error("create_project", json!({ "name": "Bad", "path": folder, "env": { "NOT-A-NAME": "x" } }));
    assert_eq!(error["kind"], "invalidEnvironmentVariableName");
    assert_eq!(home.library().projects().len(), 1, "a failed create leaves nothing behind");

    let error = agent.call_error("create_service", json!({ "project": "acme", "name": "Api", "command_line": "  " }));
    assert_eq!(error["message"], "`command_line` must be a non-empty string.");
    let error = agent
        .call_error("create_service", json!({ "project": "acme", "name": "Api", "command_line": "x", "port": 70000 }));
    assert_eq!(error["kind"], "invalidPort");
    let error = agent
        .call_error("create_service", json!({ "project": "acme", "name": "Web", "key": "web", "command_line": "x" }));
    assert_eq!(error["kind"], "duplicateServiceKey");
    let error =
        agent.call_error("update_service", json!({ "project": "acme", "service": "web", "working_directory": "/srv" }));
    assert_eq!(error["kind"], "invalidWorkingDirectory");
    let error =
        agent.call_error("update_service", json!({ "project": "acme", "service": "web", "dependencies": ["web"] }));
    assert_eq!(error["kind"], "dependencyCycle");
    let error =
        agent.call_error("update_service", json!({ "project": "acme", "service": "web", "url": "localhost:3000" }));
    assert_eq!(error["kind"], "invalidUrl");

    let added = agent.call("create_service", json!({ "project": "acme", "name": "Web", "command_line": "x" }));
    assert_eq!(added["service"]["key"], "web-2", "a Key made from the name is free");
    agent.disconnect();
}

#[test]
fn read_only_offers_only_the_tools_that_read_and_run() {
    let home = Home::new();
    home.project("Acme", "acme");
    let mut client = home.mcp_with(&["mcp", "--read-only"]);

    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> =
        tools["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "list_projects",
            "list_services",
            "status",
            "start_service",
            "stop_service",
            "start_project",
            "stop_project",
            "logs",
            "ports"
        ]
    );
    let error = client.call_error("delete_project", json!({ "project": "acme", "confirm": true }));
    assert_eq!(error["kind"], "unknownTool");
    assert_eq!(home.library().projects().len(), 1);
    client.disconnect();
}
