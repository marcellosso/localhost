//! Ported from `ProcessTreeTests.swift`. These tests launch real processes;
//! each command line is a fixture.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use localhost_core::{
    LaunchConfiguration, LocalhostError, MAX_LINE_LENGTH, OutputChannel, OutputLine, ProcessTree, StopPolicy,
    Termination,
};

/// Short timeouts so escalation tests stay fast; the escalation order is what matters.
const QUICK_STOP: StopPolicy =
    StopPolicy { interrupt_timeout: Duration::from_millis(500), terminate_timeout: Duration::from_millis(500) };
/// For tests expecting a stop to end before escalating: the timeouts only bound a failure, and a
/// busy machine (CI, the whole suite in parallel) can take longer than `QUICK_STOP` allows.
const PATIENT_STOP: StopPolicy =
    StopPolicy { interrupt_timeout: Duration::from_secs(30), terminate_timeout: Duration::from_secs(30) };

fn current_environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

struct Launch<'a> {
    command_line: &'a str,
    directory: &'a Path,
    environment: BTreeMap<String, String>,
    shell: &'a str,
    stop_policy: StopPolicy,
}

impl<'a> Launch<'a> {
    fn new(command_line: &'a str) -> Launch<'a> {
        Launch {
            command_line,
            directory: Path::new("/tmp"),
            environment: current_environment(),
            shell: "/bin/sh",
            stop_policy: QUICK_STOP,
        }
    }

    fn run(self) -> Result<ProcessTree, LocalhostError> {
        let configuration = LaunchConfiguration {
            command_line: self.command_line.into(),
            working_directory: self.directory.into(),
            environment: self.environment,
            shell: self.shell.into(),
        };
        ProcessTree::launch(&configuration, self.stop_policy)
    }
}

fn launch(command_line: &str) -> ProcessTree {
    Launch::new(command_line).run().unwrap()
}

/// Reads output lines until `count` have arrived.
async fn first_lines(count: usize, tree: &ProcessTree) -> Vec<OutputLine> {
    let mut lines = Vec::new();
    while lines.len() < count {
        let Some(batch) = tree.next_output().await else { break };
        lines.extend(batch);
    }
    lines.truncate(count);
    lines
}

async fn all_lines(tree: &ProcessTree) -> Vec<OutputLine> {
    let mut lines = Vec::new();
    while let Some(batch) = tree.next_output().await {
        lines.extend(batch);
    }
    lines
}

fn texts(lines: &[OutputLine]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

fn is_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 || errno() == libc::EPERM }
}

fn process_group_exists(pgid: i32) -> bool {
    is_alive(-pgid)
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// A command line whose shell leaves behind a subshell that ignores SIGINT
/// and SIGTERM. The parent runs `then_parent` only once the subshell's trap
/// is in place, so the fixture can't race the signals.
fn leftover_ignoring_signals(then_parent: &str) -> String {
    let marker = std::env::temp_dir().join(format!("localhost-ready-{}", std::process::id()));
    let marker = format!("{}-{}", marker.display(), rand_suffix());
    format!(
        "(trap '' INT TERM; touch '{marker}'; sleep 1000) & \
         while [ ! -e '{marker}' ]; do sleep 0.01; done; rm '{marker}'; {then_parent}"
    )
}

fn rand_suffix() -> String {
    format!("{:?}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos())
}

// MARK: Exit codes

#[tokio::test(flavor = "multi_thread")]
async fn captures_how_the_command_ended() {
    for (command_line, expected) in [
        ("exit 0", Termination::Exited { code: 0 }),
        ("exit 1", Termination::Exited { code: 1 }),
        ("exit 42", Termination::Exited { code: 42 }),
        ("kill -TERM $$", Termination::Signaled { signal: libc::SIGTERM }),
    ] {
        let exit = launch(command_line).wait_for_exit().await;
        assert_eq!(exit.termination, expected, "{command_line}");
        assert_eq!(exit.strongest_signal_sent, None);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_command_ends_with_127_mapped_to_command_not_found() {
    let tree = launch(&format!("definitely-not-a-command-{}", rand_suffix()));
    let lines = all_lines(&tree).await;
    let exit = tree.wait_for_exit().await;

    assert_eq!(exit.termination, Termination::Exited { code: 127 });
    assert_eq!(exit.termination.launch_error(), Some(LocalhostError::CommandNotFound));
    assert!(lines.iter().any(|line| line.channel == OutputChannel::Stderr && line.text.contains("not found")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_that_is_not_executable_is_mapped_to_command_not_executable() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("script.sh"), "echo hi\n").unwrap();

    let mut launch = Launch::new("./script.sh");
    launch.directory = directory.path();
    let exit = launch.run().unwrap().wait_for_exit().await;
    assert_eq!(exit.termination.launch_error(), Some(LocalhostError::CommandNotExecutable));
}

// MARK: Launch failures

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_working_directory_fails_the_launch() {
    let missing = format!("/tmp/localhost-missing-{}", rand_suffix());
    let mut launch = Launch::new("true");
    launch.directory = Path::new(&missing);
    assert_eq!(launch.run().err(), Some(LocalhostError::WorkingDirectoryMissing(missing.clone())));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_shell_fails_the_launch() {
    let mut launch = Launch::new("true");
    launch.shell = "/no/such/shell";
    assert_eq!(launch.run().err(), Some(LocalhostError::ShellNotFound("/no/such/shell".into())));
}

// MARK: Environment and working directory

#[tokio::test(flavor = "multi_thread")]
async fn runs_in_the_working_directory_with_the_given_environment() {
    let mut launch = Launch::new(r#"pwd -P; echo "$GREETING""#);
    launch.directory = Path::new("/usr/bin");
    launch.environment = [("GREETING", "hello"), ("HOME", "/tmp"), ("PATH", "/usr/bin:/bin")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let lines = all_lines(&launch.run().unwrap()).await;
    let stdout: Vec<&str> =
        lines.iter().filter(|l| l.channel == OutputChannel::Stdout).map(|l| l.text.as_str()).collect();
    assert_eq!(stdout, ["/usr/bin", "hello"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_the_command_line_through_a_login_shell() {
    // Bash reports a login shell through `shopt`, zsh through `-o login`.
    let (shell, check) = if Path::new("/bin/zsh").exists() {
        ("/bin/zsh", "[[ -o login ]] && echo login || echo not-login")
    } else {
        ("/bin/bash", "shopt -q login_shell && echo login || echo not-login")
    };
    let mut launch = Launch::new(check);
    launch.shell = shell;
    let lines = all_lines(&launch.run().unwrap()).await;
    assert_eq!(lines.last().map(|line| line.text.as_str()), Some("login"));
}

#[tokio::test(flavor = "multi_thread")]
async fn output_that_isnt_taken_makes_the_command_wait_without_losing_lines() {
    let marker = std::env::temp_dir().join(format!("localhost-backpressure-{}", rand_suffix()));
    // 11 MB, far more than the pipes and the pending batches hold.
    let tree = launch(&format!("yes 0123456789 | head -n 1000000; echo done; touch '{}'", marker.display()));

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!marker.exists(), "the command waits while its output isn't taken");

    let lines = all_lines(&tree).await;
    assert_eq!(lines.len(), 1_000_001);
    assert!(lines[..1_000_000].iter().all(|l| l.text == "0123456789" && l.channel == OutputChannel::Stdout));
    assert_eq!(lines.last().unwrap().text, "done");
    assert!(marker.exists());
    let _ = std::fs::remove_file(marker);
}

#[tokio::test(flavor = "multi_thread")]
async fn stdin_is_empty() {
    assert_eq!(texts(&all_lines(&launch("cat; echo done")).await), ["done"]);
}

// MARK: Output

#[tokio::test(flavor = "multi_thread")]
async fn streams_stdout_and_stderr_as_separate_channels_line_by_line() {
    let lines = all_lines(&launch("echo out1; echo err1 >&2; echo out2")).await;
    let on = |channel| lines.iter().filter(|l| l.channel == channel).map(|l| l.text.as_str()).collect::<Vec<_>>();
    assert_eq!(on(OutputChannel::Stdout), ["out1", "out2"]);
    assert_eq!(on(OutputChannel::Stderr), ["err1"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn delivers_lines_as_they_are_written_not_at_exit() {
    let tree = launch("echo first; sleep 1000");
    assert_eq!(texts(&first_lines(1, &tree).await), ["first"]);
    tree.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_empty_lines_strips_carriage_returns_and_flushes_a_final_partial_line() {
    let lines = all_lines(&launch(r"printf 'a\r\n\nb\nno newline'")).await;
    assert_eq!(texts(&lines), ["a", "", "b", "no newline"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn replaces_invalid_utf8_instead_of_dropping_the_line() {
    assert_eq!(texts(&all_lines(&launch(r"printf 'caf\351\n'")).await), ["caf\u{FFFD}"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_longer_than_the_limit_is_split_without_breaking_a_character() {
    // 3-byte characters, so the 64 KB limit falls inside one of them.
    let tree = launch(r#"awk 'BEGIN { s = ""; for (i = 0; i < 50000; i++) s = s "€"; print s }'"#);
    let pieces = all_lines(&tree).await;
    let joined: String = pieces.iter().map(|piece| piece.text.as_str()).collect();

    assert!(pieces.len() > 1);
    assert!(!joined.contains('\u{FFFD}'));
    assert_eq!(joined, "€".repeat(50_000));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_line_is_cut_at_the_limit_even_when_its_end_arrives_in_a_later_read() {
    // Exactly the limit, then (in another read) the rest of the line.
    let tree = launch(r"head -c 65536 /dev/zero | tr '\0' x; sleep 0.2; printf 'yyyy\n'");
    let lines = all_lines(&tree).await;
    assert_eq!(texts(&lines), ["x".repeat(MAX_LINE_LENGTH).as_str(), "yyyy"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn timestamps_each_line() {
    let before = SystemTime::now();
    let lines = all_lines(&launch("echo hi")).await;
    let line = lines.first().unwrap();
    assert!(line.timestamp >= before && line.timestamp <= SystemTime::now());
}

// MARK: Stop

#[tokio::test(flavor = "multi_thread")]
async fn stop_kills_the_whole_tree_including_grandchildren() {
    let tree = launch("sleep 1000 & echo $!; sleep 1000 & echo $!; wait");
    let grandchildren: Vec<i32> = first_lines(2, &tree).await.iter().filter_map(|l| l.text.parse().ok()).collect();
    assert_eq!(grandchildren.len(), 2);
    assert!(grandchildren.iter().all(|&pid| is_alive(pid)));

    let exit = tree.stop().await;

    assert!(!process_group_exists(tree.pid()));
    assert!(!grandchildren.iter().any(|&pid| is_alive(pid)));
    assert!(exit.strongest_signal_sent.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_that_handles_sigint_stops_gracefully_without_escalation() {
    let mut launch = Launch::new("trap 'echo bye; exit 0' INT; echo ready; while :; do sleep 0.05; done");
    launch.stop_policy = PATIENT_STOP;
    let tree = launch.run().unwrap();
    first_lines(1, &tree).await;

    let exit = tree.stop().await;

    assert_eq!(exit.strongest_signal_sent, Some(libc::SIGINT));
    assert_eq!(exit.termination, Termination::Exited { code: 0 });
    assert!(!process_group_exists(tree.pid()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_that_ignores_sigint_and_sigterm_is_escalated_to_sigkill() {
    let tree = launch("trap '' INT TERM; echo ready; sleep 1000 & wait");
    first_lines(1, &tree).await;

    let started = Instant::now();
    let exit = tree.stop().await;

    assert_eq!(exit.strongest_signal_sent, Some(libc::SIGKILL));
    assert!(exit.was_force_killed());
    assert_eq!(exit.termination, Termination::Signaled { signal: libc::SIGKILL });
    assert!(!process_group_exists(tree.pid()));
    // SIGINT and SIGTERM each got their full timeout before SIGKILL.
    assert!(started.elapsed() >= Duration::from_millis(1000));
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_is_idempotent_and_safe_to_call_concurrently() {
    let tree = launch("echo ready; sleep 1000");
    first_lines(1, &tree).await;

    let (a, b) = tokio::join!(tree.stop(), tree.stop());
    let c = tree.stop().await;

    assert_eq!(a, b);
    assert_eq!(b, c);
    assert!(!process_group_exists(tree.pid()));
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_tree_that_already_ended_returns_its_exit_without_signalling() {
    let tree = launch("exit 3");
    let exit = tree.wait_for_exit().await;
    assert_eq!(tree.stop().await, exit);
    assert_eq!(exit.strongest_signal_sent, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn processes_left_behind_when_the_shell_exits_are_cleaned_up_starting_with_sigterm() {
    let mut launch = Launch::new("sleep 1000 & echo $!; exit 0");
    launch.stop_policy = PATIENT_STOP;
    let tree = launch.run().unwrap();
    let leftover: i32 = first_lines(1, &tree).await[0].text.parse().unwrap();

    let exit = tree.wait_for_exit().await;

    assert_eq!(exit.termination, Termination::Exited { code: 0 });
    assert_eq!(exit.strongest_signal_sent, Some(libc::SIGTERM));
    assert!(!is_alive(leftover));
    assert!(!process_group_exists(tree.pid()));
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_the_shells_exit_before_leftovers_are_cleaned_up() {
    let tree = launch(&leftover_ignoring_signals("exit 1"));

    let termination = tree.wait_for_shell_exit().await;

    assert_eq!(termination, Termination::Exited { code: 1 });
    assert!(process_group_exists(tree.pid()));
    let exit = tree.wait_for_exit().await;
    assert_eq!(exit.termination, termination);
    assert!(exit.was_force_killed());
}

#[tokio::test(flavor = "multi_thread")]
async fn output_ends_once_the_tree_has_ended() {
    let tree = launch("echo ready; sleep 1000");
    first_lines(1, &tree).await;
    tree.stop().await;

    // The stream finishes rather than hanging.
    assert!(all_lines(&tree).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_process_group_is_the_launched_shells_own() {
    let tree = launch("echo ready; sleep 1000");
    first_lines(1, &tree).await;
    unsafe {
        assert_eq!(libc::getpgid(tree.pid()), tree.pid());
        assert_eq!(libc::getsid(tree.pid()), tree.pid());
        assert_ne!(libc::getpgid(tree.pid()), libc::getpgrp());
    }
    tree.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_file_descriptor_of_ours_leaks_into_the_command() {
    // A descriptor this process opens without close-on-exec.
    let file = std::fs::File::open("/dev/null").unwrap();
    let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
    unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };

    let tree = launch(&format!("if [ -e /dev/fd/{fd} ]; then echo leaked; else echo closed; fi"));
    assert_eq!(texts(&all_lines(&tree).await), ["closed"]);
    drop(file);
}
