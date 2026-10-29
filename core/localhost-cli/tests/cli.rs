//! The `localhost` command, run as a real process against a temporary home
//! folder, so it finds its storage the way it does on a user's machine.
//! Its Services are shell command lines, stopped with Unix signals.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use localhost_core::{Id, Key, MemorySecretStore, NewService, Origin, Project, ProjectLibrary, Service};
use tempfile::TempDir;

/// How long a test waits for something before failing instead of hanging.
const PATIENCE: Duration = Duration::from_secs(20);

const SWIFT_FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../localhost-core/tests/fixtures/swift-schema-1");

/// A home folder for the command, removed when dropped.
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

    /// Where the command looks for its data under this home folder.
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

    /// A Project whose folder exists, with Services added by `services`.
    fn project(&self, name: &str, key: &str) -> Project {
        let folder = self.path().join("code").join(key);
        std::fs::create_dir_all(&folder).unwrap();
        self.library()
            .add_project(name, Key::new(key).unwrap(), folder.to_str().unwrap(), vec![], Origin::MainWindow)
            .unwrap()
    }

    fn service(&self, project: &Project, key: &str, command_line: &str, dependencies: Vec<Id>) -> Service {
        let service = NewService::new(capitalized(key), Key::new(key).unwrap(), command_line);
        self.library().add_service(project.id, service, vec![], dependencies, Origin::MainWindow).unwrap()
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_localhost"));
        command
            .args(arguments)
            .current_dir(self.path())
            .env("HOME", self.path())
            .env("XDG_DATA_HOME", self.path().join("data"))
            .env("SHELL", "/bin/sh")
            .env_remove("NO_COLOR");
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }

    /// Starts `localhost up …`, collecting its output as it comes.
    fn up(&self, arguments: &[&str]) -> Up {
        let mut child = self
            .command(&[&["up"], arguments].concat())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = Arc::new(Mutex::new(String::new()));
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let output = output.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let mut output = output.lock().unwrap();
                    output.push_str(&line);
                    output.push('\n');
                }
            });
        }
        Up { child, output }
    }

    /// Waits for the file a Service writes when it has started, and returns
    /// its contents: the Service shell's pid, which is its process group.
    fn wait_for_pid(&self, name: &str) -> i32 {
        let path = self.path().join(name);
        wait_until(|| std::fs::read_to_string(&path).is_ok_and(|text| text.ends_with('\n')), name);
        std::fs::read_to_string(&path).unwrap().trim().parse().unwrap()
    }
}

/// A running `localhost up`.
struct Up {
    child: Child,
    output: Arc<Mutex<String>>,
}

impl Up {
    fn output(&self) -> String {
        self.output.lock().unwrap().clone()
    }

    fn wait_for_output(&self, text: &str) {
        wait_until(|| self.output().contains(text), &format!("{text:?} in:\n{}", self.output()));
    }

    fn signal(&self, signal: i32) {
        unsafe { libc::kill(self.child.id() as i32, signal) };
    }

    /// Waits for the command to exit, returning its exit code and output.
    fn wait(mut self) -> (i32, String) {
        let (sender, receiver) = mpsc::channel();
        let pid = self.child.id();
        std::thread::spawn(move || sender.send(self.child.wait().unwrap()).unwrap());
        let Ok(status) = receiver.recv_timeout(PATIENCE) else {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            panic!("`localhost up` did not exit:\n{}", self.output.lock().unwrap());
        };
        // Let the reader threads take the last lines.
        std::thread::sleep(Duration::from_millis(100));
        let output = self.output.lock().unwrap().clone();
        (status.code().unwrap_or(-1), output)
    }
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map(|first| first.to_uppercase().chain(characters).collect()).unwrap_or_default()
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A command line that records its shell's pid in `name`, then stays up
/// with a grandchild, as most dev servers do.
fn long_running(home: &Home, name: &str) -> String {
    format!("echo started {name}; sleep 1000 & echo $$ > '{}'; wait", home.path().join(name).display())
}

// MARK: Usage

#[test]
fn without_a_command_it_prints_usage_and_fails() {
    let output = Home::new().run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Usage: localhost <command>"), "{}", stderr(&output));
}

#[test]
fn help_lists_every_command() {
    let output = Home::new().run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    for command in ["projects", "status <project>", "logs <project> [service]", "up <project> [service…]"] {
        assert!(stdout(&output).contains(command), "{command} in {}", stdout(&output));
    }
}

#[test]
fn version_prints_the_cores_version() {
    let output = Home::new().run(&["--version"]);
    assert_eq!(stdout(&output), format!("localhost {}\n", localhost_core::VERSION));
}

#[test]
fn an_unknown_command_or_a_missing_argument_is_a_usage_error() {
    let home = Home::new();
    for arguments in [&["start"][..], &["status"], &["logs", "a", "b", "c"], &["projects", "extra"], &["up"]] {
        let output = home.run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(stderr(&output).contains("localhost --help"), "{arguments:?}: {}", stderr(&output));
    }
}

// MARK: projects

#[test]
fn projects_lists_every_project_in_sort_order() {
    let home = Home::new();
    let acme = home.project("Acme Shop", "acme");
    home.service(&acme, "web", "true", vec![]);
    home.project("Blog", "blog");

    let output = home.run(&["projects"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let lines: Vec<String> =
        stdout(&output).lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    assert_eq!(lines[0], "KEY NAME SERVICES PATH");
    assert_eq!(lines[1], "acme Acme Shop 1 ~/code/acme");
    assert_eq!(lines[2], "blog Blog 0 ~/code/blog");
    assert_eq!(lines.len(), 3);
}

#[test]
fn projects_reads_the_data_the_swift_app_wrote() {
    let home = Home::new();
    let projects = home.storage_root().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    for entry in std::fs::read_dir(Path::new(SWIFT_FIXTURE).join("projects")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), projects.join(entry.file_name())).unwrap();
    }

    let output = home.run(&["projects"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let listed = stdout(&output);
    let keys: Vec<&str> = listed.lines().skip(1).filter_map(|line| line.split_whitespace().next()).collect();
    assert_eq!(keys, ["acme-shop", "empty"]);
    assert!(stdout(&output).contains("Acme Shop"));
}

#[test]
fn projects_without_any_says_so() {
    let output = Home::new().run(&["projects"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("No Projects yet"), "{}", stderr(&output));
}

#[test]
fn projects_reports_unreadable_files_and_lists_the_rest() {
    let home = Home::new();
    home.project("Blog", "blog");
    std::fs::write(home.storage_root().join("projects/damaged.json"), "{").unwrap();

    let output = home.run(&["projects"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("blog"));
    assert!(stderr(&output).contains("damaged.json"), "{}", stderr(&output));
}

// MARK: Names

#[test]
fn names_match_by_key_name_or_letters_in_order() {
    let home = Home::new();
    home.project("Acme Shop", "acme");
    home.project("Blog", "blog");
    for query in ["acme", "Acme Shop", "ashop", "ACME"] {
        let output = home.run(&["status", query]);
        assert_eq!(output.status.code(), Some(0), "{query}: {}", stderr(&output));
        assert!(stdout(&output).starts_with("Acme Shop"), "{query}: {}", stdout(&output));
    }
}

#[test]
fn an_unknown_project_lists_the_projects() {
    let home = Home::new();
    home.project("Acme Shop", "acme");
    home.project("Blog", "blog");

    let output = home.run(&["status", "zzz"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output), "No Project matches “zzz”. Projects: acme, blog.\n");
}

#[test]
fn an_ambiguous_name_lists_the_candidates() {
    let home = Home::new();
    home.project("Shop One", "shop-one");
    home.project("Shop Two", "shop-two");

    let output = home.run(&["status", "shop"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "“shop” matches more than one Project: shop-one (Shop One), shop-two (Shop Two). Use its Key.\n"
    );
}

// MARK: status

#[test]
fn status_shows_each_service_and_which_are_running() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db.pid"), vec![]);
    home.service(&project, "web", "true", vec![db.id]);
    let changes = localhost_core::ServiceChanges { port: Some(Some(54_321)), ..Default::default() };
    home.library().edit_service(db.id, changes, Origin::MainWindow).unwrap();

    let output = home.run(&["status", "acme"]);
    let table: Vec<Vec<String>> = stdout(&output)
        .lines()
        .skip(1)
        .map(|line| line.split("  ").filter(|c| !c.is_empty()).map(|c| c.trim().to_string()).collect())
        .collect();
    assert_eq!(table[0], ["SERVICE", "NAME", "STATE", "PORT", "PID", "UP"]);
    assert_eq!(table[1], ["db", "Db", "not running", "54321"]);
    assert_eq!(table[2], ["web", "Web", "not running"]);

    let up = home.up(&["acme", "db"]);
    let pid = home.wait_for_pid("db.pid");

    let output = home.run(&["status", "acme"]);
    let db_line = stdout(&output).lines().find(|line| line.starts_with("db")).unwrap().to_string();
    let db_columns: Vec<&str> = db_line.split_whitespace().collect();
    assert_eq!(&db_columns[..5], ["db", "Db", "running", "54321", &pid.to_string()], "{db_line}");

    up.signal(libc::SIGINT);
    up.wait();
}

// MARK: up

#[test]
fn up_starts_services_in_dependency_order_and_ctrl_c_leaves_no_process_behind() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    let order = home.path().join("order");
    let record =
        |name: &str| format!("echo {name} >> '{}'; {}", order.display(), long_running(&home, &format!("{name}.pid")));
    // Added dependents first, so only the dependency order puts db first.
    let web = home.service(&project, "web", &record("web"), vec![]);
    let api = home.service(&project, "api", &record("api"), vec![]);
    let db = home.service(&project, "db", &record("db"), vec![]);
    let library = home.library();
    for (service, dependencies) in [(&web, vec![api.id]), (&api, vec![db.id])] {
        let changes = localhost_core::ServiceChanges { dependencies: Some(dependencies), ..Default::default() };
        library.edit_service(service.id, changes, Origin::MainWindow).unwrap();
    }

    let up = home.up(&["acme"]);
    let groups: Vec<i32> = ["db.pid", "api.pid", "web.pid"].iter().map(|name| home.wait_for_pid(name)).collect();
    up.wait_for_output("web | started web");
    assert_eq!(std::fs::read_to_string(&order).unwrap(), "db\napi\nweb\n");

    up.signal(libc::SIGINT);
    let (code, output) = up.wait();

    assert_eq!(code, 0, "{output}");
    for group in groups {
        assert!(!process_group_exists(group), "process group {group} survived:\n{output}");
    }
    assert!(output.contains("db  | started db"), "{output}");
    assert!(output.contains("web ▸ waiting for api"), "{output}");
    assert!(output.contains("Stopping"), "{output}");
    // Every Run record is gone: the next session finds no Orphans.
    let runs = home.storage_root().join("runs");
    assert_eq!(std::fs::read_dir(&runs).map(|entries| entries.count()).unwrap_or(0), 0);
}

#[test]
fn up_stops_everything_when_terminated_or_hung_up_too() {
    for signal in [libc::SIGTERM, libc::SIGHUP] {
        let home = Home::new();
        let project = home.project("Acme Shop", "acme");
        home.service(&project, "web", &long_running(&home, "web.pid"), vec![]);

        let up = home.up(&["acme"]);
        let group = home.wait_for_pid("web.pid");
        up.signal(signal);
        let (code, output) = up.wait();

        assert_eq!(code, 0, "{output}");
        assert!(!process_group_exists(group), "signal {signal}:\n{output}");
    }
}

#[test]
fn up_with_services_starts_only_those_and_their_dependencies() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    let db = home.service(&project, "db", &long_running(&home, "db.pid"), vec![]);
    home.service(&project, "web", &long_running(&home, "web.pid"), vec![db.id]);
    home.service(&project, "worker", &long_running(&home, "worker.pid"), vec![]);

    let up = home.up(&["acme", "web"]);
    let groups = [home.wait_for_pid("db.pid"), home.wait_for_pid("web.pid")];
    up.signal(libc::SIGINT);
    let (code, output) = up.wait();

    assert_eq!(code, 0, "{output}");
    assert!(!home.path().join("worker.pid").exists(), "{output}");
    assert!(groups.iter().all(|&group| !process_group_exists(group)));
}

#[test]
fn up_ends_by_itself_once_every_service_has_ended() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "migrate", "echo migrated", vec![]);

    let (code, output) = home.up(&["acme"]).wait();

    assert_eq!(code, 0, "{output}");
    assert!(output.contains("migrate | migrated"), "{output}");
    assert!(output.contains("migrate | Exited with code 0"), "{output}");
}

#[test]
fn up_fails_when_a_service_failed() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "broken", "echo nope >&2; exit 3", vec![]);
    home.service(&project, "missing", "no-such-command-anywhere", vec![]);

    let (code, output) = home.up(&["acme"]).wait();

    assert_eq!(code, 1, "{output}");
    assert!(output.contains("broken  | nope"), "{output}");
    assert!(output.contains("broken  ▸ failed: Exited with code 3"), "{output}");
    assert!(output.contains("missing ▸ failed:"), "{output}");
}

#[test]
fn up_refuses_services_another_session_is_running() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "db", &long_running(&home, "db.pid"), vec![]);

    let first = home.up(&["acme"]);
    let group = home.wait_for_pid("db.pid");
    let second = home.run(&["up", "acme"]);

    assert_eq!(second.status.code(), Some(1));
    assert_eq!(
        stderr(&second),
        format!("db is already running (pid {group}) in another localhost session. Stop it there first.\n")
    );
    first.signal(libc::SIGINT);
    first.wait();
    assert!(!process_group_exists(group));
}

#[test]
fn up_with_an_unknown_service_lists_the_services() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "db", "true", vec![]);
    home.service(&project, "web", "true", vec![]);

    let output = home.run(&["up", "acme", "zzz"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr(&output), "No Service of Acme Shop matches “zzz”. Services: db, web.\n");
}

// MARK: logs

#[test]
fn logs_print_the_latest_run_of_each_service_merged_by_time() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "db", "echo db one; sleep 0.3; echo db two", vec![]);
    home.service(&project, "web", "sleep 0.15; echo web one", vec![]);
    let (code, output) = home.up(&["acme"]).wait();
    assert_eq!(code, 0, "{output}");

    let output = home.run(&["logs", "acme"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed = stdout(&output);
    let lines: Vec<&str> = printed.lines().filter(|line| !line.contains("Exited")).collect();
    assert_eq!(lines, ["db  | db one", "web | web one", "db  | db two"]);

    let output = home.run(&["logs", "acme", "web"]);
    assert_eq!(stdout(&output), "web one\nExited with code 0\n");
}

#[test]
fn logs_of_a_project_that_never_ran_say_so() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "web", "true", vec![]);

    let output = home.run(&["logs", "acme"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "No logs yet for Acme Shop.\n");
}

#[test]
fn logs_leave_out_colours_when_not_writing_to_a_terminal() {
    let home = Home::new();
    let project = home.project("Acme Shop", "acme");
    home.service(&project, "web", r"printf '\033[31mred\033[0m\n'", vec![]);
    home.up(&["acme"]).wait();

    let output = home.run(&["logs", "acme", "web"]);

    assert_eq!(stdout(&output).lines().next(), Some("red"));
}
