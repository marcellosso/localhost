//! A Run's process tree on Windows: `cmd.exe /s /c` in a kill-on-close Job
//! Object, stopped with Ctrl-Break and then by terminating the job. These
//! tests launch real processes; Python scripts are the
//! fixtures that trap Ctrl-Break and spawn grandchildren.
#![cfg(windows)]

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use localhost_core::{
    LaunchConfiguration, LocalhostError, OutputChannel, OutputLine, ProcessTree, RunEnvironment, StopPolicy,
    Termination,
};
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, TerminateProcess,
};

const SIGINT: i32 = 2;
const SIGKILL: i32 = 9;

const QUICK_STOP: StopPolicy =
    StopPolicy { interrupt_timeout: Duration::from_millis(500), terminate_timeout: Duration::from_millis(500) };
const PATIENT_STOP: StopPolicy =
    StopPolicy { interrupt_timeout: Duration::from_secs(30), terminate_timeout: Duration::from_secs(30) };

/// This process's environment, so `python` resolves as it does for the test runner.
fn environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

fn launch_in(directory: &Path, command_line: &str, stop_policy: StopPolicy) -> Result<ProcessTree, LocalhostError> {
    let configuration = LaunchConfiguration::new(command_line, directory, environment());
    ProcessTree::launch(&configuration, stop_policy)
}

fn launch(command_line: &str) -> ProcessTree {
    launch_in(&std::env::temp_dir(), command_line, QUICK_STOP).unwrap()
}

/// A folder holding `script.py`, run as `python script.py`.
fn python(script: &str) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("script.py"), script).unwrap();
    folder
}

async fn all_lines(tree: &ProcessTree) -> Vec<OutputLine> {
    let mut lines = Vec::new();
    while let Some(batch) = tree.next_output().await {
        lines.extend(batch);
    }
    lines
}

/// Reads output until a line starts with `prefix`, and returns the rest of it.
async fn line_starting_with(prefix: &str, tree: &ProcessTree) -> String {
    let wait = async {
        loop {
            let batch = tree.next_output().await.expect("the tree prints the line before it ends");
            if let Some(line) = batch.iter().find(|line| line.text.starts_with(prefix)) {
                return line.text[prefix.len()..].trim().to_string();
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(30), wait).await.expect("the line within 30s")
}

fn is_alive(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut code = 0;
        let alive = GetExitCodeProcess(process, &mut code) != 0 && code == STILL_ACTIVE as u32;
        CloseHandle(process);
        alive
    }
}

async fn wait_until_dead(pid: u32) {
    for _ in 0..500 {
        if !is_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process {pid} is still alive");
}

/// Prints its own pid and a grandchild's, ignores Ctrl-Break in both, and sleeps.
const IGNORES_CTRL_BREAK: &str = r#"
import os, signal, subprocess, sys, time
signal.signal(signal.SIGBREAK, signal.SIG_IGN)
child = subprocess.Popen([sys.executable, "-c", "import signal, time; signal.signal(signal.SIGBREAK, signal.SIG_IGN); open('ready', 'w').close(); time.sleep(1000)"])
while not os.path.exists("ready"): time.sleep(0.01)
print("pid", os.getpid(), flush=True)
print("grandchild", child.pid, flush=True)
time.sleep(1000)
"#;

// MARK: Exit codes and output

#[tokio::test(flavor = "multi_thread")]
async fn captures_the_exit_code() {
    for (command_line, code) in [("exit /b 0", 0), ("exit /b 1", 1), ("exit /b 42", 42)] {
        let exit = launch(command_line).wait_for_exit().await;
        assert_eq!(exit.termination, Termination::Exited { code }, "{command_line}");
        assert_eq!(exit.strongest_signal_sent, None);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_command_exits_with_1_and_says_so_on_stderr() {
    let tree = launch("definitely-not-a-command-localhost");
    let lines = all_lines(&tree).await;
    let exit = tree.wait_for_exit().await;
    assert_eq!(exit.termination, Termination::Exited { code: 1 });
    assert!(
        lines.iter().any(|line| line.channel == OutputChannel::Stderr
            && line.text.contains("is not recognized as an internal or external command")),
        "{lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stdout_and_stderr_arrive_separately_without_carriage_returns() {
    let tree = launch("echo out& echo err 1>&2");
    let lines = all_lines(&tree).await;
    let text = |channel| {
        lines.iter().filter(|line| line.channel == channel).map(|line| line.text.trim_end()).collect::<Vec<_>>()
    };
    assert_eq!(text(OutputChannel::Stdout), ["out"]);
    assert_eq!(text(OutputChannel::Stderr), ["err"]);
    assert!(lines.iter().all(|line| !line.text.contains('\r')));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_command_line_reaches_cmd_verbatim() {
    let tree = launch(r#"echo "quoted text" & echo %LOCALHOST_TEST_VALUE%"#);
    let lines = all_lines(&tree).await;
    let texts: Vec<_> = lines.iter().map(|line| line.text.trim_end()).collect();
    // Quotes are kept, and an undefined variable stays as written.
    assert_eq!(texts, [r#""quoted text""#, "%LOCALHOST_TEST_VALUE%"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_with_the_given_environment_and_working_directory() {
    let folder = tempfile::tempdir().unwrap();
    let mut environment = environment();
    environment.insert("LOCALHOST_TEST_VALUE".into(), "from localhost".into());
    let configuration = LaunchConfiguration::new("echo %LOCALHOST_TEST_VALUE%& cd", folder.path(), environment);
    let tree = ProcessTree::launch(&configuration, QUICK_STOP).unwrap();
    let lines = all_lines(&tree).await;
    let texts: Vec<_> = lines.iter().map(|line| line.text.trim_end().to_string()).collect();
    assert_eq!(texts[0], "from localhost");
    let name = folder.path().file_name().unwrap().to_string_lossy().to_lowercase();
    assert!(texts[1].to_lowercase().ends_with(&name), "{texts:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shell_is_cmd() {
    let configuration = LaunchConfiguration::new("exit /b 0", std::env::temp_dir(), RunEnvironment::current());
    assert!(configuration.shell.to_lowercase().ends_with("cmd.exe"), "{}", configuration.shell);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_working_directory_or_shell_fails_before_launching() {
    let missing = std::env::temp_dir().join("localhost-definitely-missing");
    assert!(matches!(launch_in(&missing, "exit /b 0", QUICK_STOP), Err(LocalhostError::WorkingDirectoryMissing(_))));
    let mut configuration = LaunchConfiguration::new("exit /b 0", std::env::temp_dir(), environment());
    configuration.shell = r"C:\definitely\missing\cmd.exe".into();
    assert!(matches!(ProcessTree::launch(&configuration, QUICK_STOP), Err(LocalhostError::ShellNotFound(_))));
}

// MARK: Stopping

#[tokio::test(flavor = "multi_thread")]
async fn ctrl_break_stops_a_command_and_its_grandchildren() {
    let folder = python(
        r#"
import os, subprocess, sys, time
# The grandchild says it's ready once it runs, so Ctrl-Break can't arrive while it is starting.
child = subprocess.Popen([sys.executable, "-c", "open('ready', 'w').close(); import time; time.sleep(1000)"])
while not os.path.exists("ready"): time.sleep(0.01)
print("grandchild", child.pid, flush=True)
time.sleep(1000)
"#,
    );
    let tree = launch_in(folder.path(), "python script.py", PATIENT_STOP).unwrap();
    let grandchild: u32 = line_starting_with("grandchild", &tree).await.parse().unwrap();

    let exit = tree.stop().await;
    assert_eq!(exit.strongest_signal_sent, Some(SIGINT));
    assert!(!exit.was_force_killed());
    wait_until_dead(grandchild).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tree_ignoring_ctrl_break_is_terminated_with_its_job() {
    let folder = python(IGNORES_CTRL_BREAK);
    let tree = launch_in(folder.path(), "python script.py", QUICK_STOP).unwrap();
    let grandchild: u32 = line_starting_with("grandchild", &tree).await.parse().unwrap();

    let exit = tree.stop().await;
    assert_eq!(exit.strongest_signal_sent, Some(SIGKILL));
    assert!(exit.was_force_killed());
    assert!(!is_alive(grandchild));
}

#[tokio::test(flavor = "multi_thread")]
async fn what_the_shell_leaves_behind_is_terminated_when_it_exits() {
    // The shell waits until the leftover has said its pid, then exits.
    let folder = python(
        "import os, time\nprint('leftover', os.getpid(), flush=True)\nopen('ready', 'w').close()\ntime.sleep(1000)\n",
    );
    std::fs::write(
        folder.path().join("wait.py"),
        "import os, time\nwhile not os.path.exists('ready'): time.sleep(0.01)\n",
    )
    .unwrap();
    let tree = launch_in(folder.path(), "start /b python script.py & python wait.py & exit /b 3", QUICK_STOP).unwrap();
    let leftover: u32 = line_starting_with("leftover", &tree).await.parse().unwrap();

    let exit = tree.wait_for_exit().await;
    assert_eq!(exit.termination, Termination::Exited { code: 3 });
    assert_eq!(exit.strongest_signal_sent, Some(SIGKILL));
    assert!(!is_alive(leftover));
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_twice_concurrently_ends_once() {
    let folder = python("import time\nprint('ready', flush=True)\ntime.sleep(1000)\n");
    let tree = launch_in(folder.path(), "python script.py", PATIENT_STOP).unwrap();
    line_starting_with("ready", &tree).await;
    let (first, second) = tokio::join!(tree.stop(), tree.stop());
    assert_eq!(first, second);
}

// MARK: When localhost's process ends

/// Not a test of its own: `force_killing_localhost_ends_every_run` runs this
/// in a child process standing in for the app, which launches a Run and then
/// waits to be killed.
#[tokio::test(flavor = "multi_thread")]
async fn helper_launches_a_run_and_waits() {
    let Ok(folder) = std::env::var("LOCALHOST_TEST_HELPER_FOLDER") else { return };
    let tree = launch_in(Path::new(&folder), "python script.py", QUICK_STOP).unwrap();
    let grandchild = line_starting_with("grandchild", &tree).await;
    println!("helper-grandchild {grandchild}");
    tokio::time::sleep(Duration::from_secs(1000)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn force_killing_localhost_ends_every_run() {
    use std::io::{BufRead, BufReader};

    let folder = python(IGNORES_CTRL_BREAK);
    let mut helper = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["helper_launches_a_run_and_waits", "--exact", "--nocapture"])
        .env("LOCALHOST_TEST_HELPER_FOLDER", folder.path())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // libtest's own output may share the line, so look for the marker anywhere in it.
    let stdout = helper.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((_, pid)) = line.split_once("helper-grandchild ") {
                let _ = sender.send(pid.trim().parse::<u32>().unwrap());
                return;
            }
        }
    });
    let grandchild = receiver.recv_timeout(Duration::from_secs(60));
    let alive = grandchild.as_ref().is_ok_and(|&pid| is_alive(pid));

    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, helper.id());
        TerminateProcess(process, 1);
        CloseHandle(process);
    }
    let _ = helper.wait();
    let grandchild = grandchild.expect("the helper's Run printed its grandchild within 60s");
    assert!(alive, "the grandchild was running before the helper was killed");
    wait_until_dead(grandchild).await;
}
