//! `localhost mcp`, run as a real process against a temporary home folder and
//! driven over its standard input and output as a coding agent's client would.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use localhost_core::{Id, Key, MemorySecretStore, NewService, Origin, Project, ProjectLibrary, Service};
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
            .env("SHELL", "/bin/sh");
        command
    }

    fn mcp(&self) -> Client {
        let mut child = self
            .command(&["mcp"])
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
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("`localhost mcp` did not exit after its input closed");
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

fn process_group_exists(group: i32) -> bool {
    unsafe { libc::kill(-group, 0) == 0 }
}

/// A command line that records its shell's pid in `name`, then stays up
/// with a grandchild, as most dev servers do.
fn long_running(home: &Home, name: &str) -> String {
    format!("echo started {name}; sleep 1000 & echo $$ > '{}'; wait", home.path().join(name).display())
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
    assert_eq!(service(&status, "web")["session"], "agent");

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

    // Disconnecting stops everything this session started.
    assert_eq!(client.disconnect(), 0);
    assert!(!process_group_exists(db_group));
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
