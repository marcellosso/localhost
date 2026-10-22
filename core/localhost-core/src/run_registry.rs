//! Records every active Run on disk, one file per Run at
//! `<root>/runs/<RUN-ID>.json`, so that a later session can find its Orphans.
//! The record format is the one earlier (Swift) versions of localhost wrote.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::platform::signal::{SIGINT, SIGKILL, SIGTERM};
use crate::platform::{self, ProcessStartTime, process_group_exists};
use crate::process::StopPolicy;
use crate::{Id, Project, service_label};

const SCHEMA_VERSION: i64 = 1;
/// Seconds from the Unix epoch to 2001-01-01, the reference date `startedAt`
/// is encoded against.
const REFERENCE_DATE_OFFSET: f64 = 978_307_200.0;

/// A process tree left over from a Run whose localhost session ended
/// unexpectedly (crash, force-quit). Orphans are never re-adopted as
/// Running: they can only be stopped or left alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Orphan {
    pub run_id: Id,
    pub service_id: Id,
    /// The Run's process group (also the pid of its shell).
    pub process_group_id: i32,
    pub started_at: SystemTime,
    pub(crate) start_time: ProcessStartTime,
}

impl Orphan {
    /// What the Orphans alert lists, e.g. "My SaaS › API".
    pub fn description(&self, projects: &[Project]) -> String {
        service_label(projects, self.service_id)
            .unwrap_or_else(|| format!("A removed service (process group {})", self.process_group_id))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    schema_version: i64,
    #[serde(rename = "runID")]
    run_id: Id,
    #[serde(rename = "serviceID")]
    service_id: Id,
    pid: i32,
    start_time: ProcessStartTime,
    /// Seconds since 2001-01-01, as Swift's `JSONEncoder` writes a `Date`.
    started_at: f64,
}

/// Call `orphans()` at launch: Runs recorded by this session are never reported.
#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct RunRegistry {
    directory: PathBuf,
    session_run_ids: Mutex<HashSet<Id>>,
}

impl RunRegistry {
    pub fn new(root: &Path) -> RunRegistry {
        RunRegistry { directory: root.join("runs"), session_run_ids: Mutex::new(HashSet::new()) }
    }

    fn file_path(&self, run_id: Id) -> PathBuf {
        self.directory.join(format!("{run_id}.json"))
    }

    // MARK: Recording (by the ServiceManager)

    pub(crate) fn record(
        &self,
        run_id: Id,
        service_id: Id,
        started_at: SystemTime,
        pid: i32,
        start_time: ProcessStartTime,
    ) {
        self.session_run_ids.lock().unwrap().insert(run_id);
        let since_epoch = started_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
        let record = Record {
            schema_version: SCHEMA_VERSION,
            run_id,
            service_id,
            pid,
            start_time,
            started_at: since_epoch - REFERENCE_DATE_OFFSET,
        };
        // A missing record only means this Run can't be offered as an Orphan
        // after a crash; it must never stop the Run itself.
        let _ = fs::create_dir_all(&self.directory);
        if let Ok(data) = serde_json::to_vec(&record) {
            let temporary = self.directory.join(format!(".{run_id}.tmp"));
            if fs::write(&temporary, data).is_ok() {
                let _ = fs::rename(&temporary, self.file_path(run_id));
            }
        }
    }

    pub(crate) fn remove(&self, run_id: Id) {
        let _ = fs::remove_file(self.file_path(run_id));
        self.session_run_ids.lock().unwrap().remove(&run_id);
    }

    // MARK: Orphans

    /// Runs recorded by earlier sessions whose process trees are still alive,
    /// oldest first. Records of trees that are gone are deleted.
    pub fn orphans(&self) -> Vec<Orphan> {
        let session_run_ids = self.session_run_ids.lock().unwrap().clone();
        let Ok(entries) = fs::read_dir(&self.directory) else { return Vec::new() };
        let mut orphans = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Some(record) = fs::read(&path).ok().and_then(|data| serde_json::from_slice::<Record>(&data).ok())
            else {
                let _ = fs::remove_file(&path);
                continue;
            };
            if session_run_ids.contains(&record.run_id) {
                continue;
            }
            let started_at = UNIX_EPOCH + Duration::from_secs_f64((record.started_at + REFERENCE_DATE_OFFSET).max(0.0));
            let orphan = Orphan {
                run_id: record.run_id,
                service_id: record.service_id,
                process_group_id: record.pid,
                started_at,
                start_time: record.start_time,
            };
            if is_alive(&orphan) {
                orphans.push(orphan);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
        orphans.sort_by_key(|orphan| orphan.started_at);
        orphans
    }

    /// Stops the Orphans' process groups in parallel (SIGINT, then SIGTERM,
    /// then SIGKILL) and forgets them.
    pub async fn stop_orphans(&self, orphans: &[Orphan], policy: StopPolicy) {
        let stops = orphans.iter().map(|orphan| async move {
            stop(orphan, policy).await;
            self.remove(orphan.run_id);
        });
        futures_join_all(stops).await;
    }

    pub fn leave_orphans(&self, orphans: &[Orphan]) {
        for orphan in orphans {
            self.remove(orphan.run_id);
        }
    }
}

/// Whether the Orphan's process group still exists and is still the one
/// localhost launched. While a group exists its id can't be reused as a pid,
/// so if the leader is gone the group is still ours; if the leader is alive,
/// its start time must match the recorded one.
fn is_alive(orphan: &Orphan) -> bool {
    if !process_group_exists(orphan.process_group_id) {
        return false;
    }
    ProcessStartTime::of(orphan.process_group_id).is_none_or(|leader| leader == orphan.start_time)
}

async fn stop(orphan: &Orphan, policy: StopPolicy) {
    let steps =
        [(SIGINT, policy.interrupt_timeout), (SIGTERM, policy.terminate_timeout), (SIGKILL, Duration::from_secs(5))];
    for (signal, timeout) in steps {
        if !is_alive(orphan) {
            return;
        }
        platform::signal_process_group(orphan.process_group_id, signal);
        let deadline = Instant::now() + timeout;
        while is_alive(orphan) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// Runs the futures concurrently on this task and waits for all of them.
async fn futures_join_all<F: std::future::Future<Output = ()>>(futures: impl IntoIterator<Item = F>) {
    let mut set: Vec<std::pin::Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
    std::future::poll_fn(|context| {
        set.retain_mut(|future| future.as_mut().poll(context).is_pending());
        if set.is_empty() { std::task::Poll::Ready(()) } else { std::task::Poll::Pending }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/swift-schema-1");

    #[test]
    fn reads_a_record_the_swift_core_wrote() {
        let data = fs::read(Path::new(FIXTURE).join("runs/11111111-2222-4333-8444-555555555555.json")).unwrap();
        let record: Record = serde_json::from_slice(&data).unwrap();
        assert_eq!(record.run_id.to_string(), "11111111-2222-4333-8444-555555555555");
        assert_eq!(record.pid, 4242);
        assert_eq!(record.start_time, ProcessStartTime { seconds: 1_760_000_000, microseconds: 123_456 });
        // Written as Date(timeIntervalSince1970: 1_760_000_000.5).
        assert_eq!(record.started_at + REFERENCE_DATE_OFFSET, 1_760_000_000.5);
    }

    #[test]
    fn records_the_pid_and_the_process_start_time() {
        let root = tempfile::tempdir().unwrap();
        let registry = RunRegistry::new(root.path());
        let pid = std::process::id() as i32;
        let (run_id, service_id) = (Id::new(), Id::new());
        registry.record(run_id, service_id, SystemTime::now(), pid, ProcessStartTime::of(pid).unwrap());

        let data = fs::read(root.path().join(format!("runs/{run_id}.json"))).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["pid"], pid);
        assert_eq!(json["serviceID"], service_id.to_string());
    }

    #[test]
    fn a_record_whose_process_group_is_gone_is_not_an_orphan_and_is_removed() {
        let root = tempfile::tempdir().unwrap();
        // A pid that is not running: a process we started and reaped.
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        RunRegistry::new(root.path()).record(
            Id::new(),
            Id::new(),
            SystemTime::now(),
            pid,
            ProcessStartTime { seconds: 1, microseconds: 0 },
        );

        assert!(RunRegistry::new(root.path()).orphans().is_empty());
        assert_eq!(fs::read_dir(root.path().join("runs")).unwrap().count(), 0);
    }

    #[cfg(unix)]
    fn quick_stop() -> StopPolicy {
        StopPolicy { interrupt_timeout: Duration::from_millis(300), terminate_timeout: Duration::from_millis(300) }
    }

    #[cfg(unix)]
    fn launch(root: &Path, command_line: &str, policy: StopPolicy) -> crate::ProcessTree {
        let configuration = crate::LaunchConfiguration {
            command_line: command_line.into(),
            working_directory: root.into(),
            environment: [("PATH".to_string(), "/usr/bin:/bin".to_string())].into(),
            shell: "/bin/sh".into(),
        };
        crate::ProcessTree::launch(&configuration, policy).unwrap()
    }

    #[cfg(unix)]
    /// Records a tree as a previous session's Run.
    fn record(root: &Path, tree: &crate::ProcessTree) -> Id {
        let run_id = Id::new();
        let start_time = ProcessStartTime::of(tree.pid()).unwrap();
        RunRegistry::new(root).record(run_id, Id::new(), SystemTime::now(), tree.pid(), start_time);
        run_id
    }

    #[tokio::test(flavor = "multi_thread")]
    #[cfg(unix)]
    async fn a_new_session_finds_the_previous_sessions_surviving_runs_but_not_its_own() {
        let root = tempfile::tempdir().unwrap();
        let tree = launch(root.path(), "sleep 1000", quick_stop());
        let run_id = record(root.path(), &tree);

        let orphans = RunRegistry::new(root.path()).orphans();
        assert_eq!(orphans.iter().map(|o| o.run_id).collect::<Vec<_>>(), [run_id]);

        // The session that recorded it doesn't see its own Run.
        let own = RunRegistry::new(root.path());
        own.record(Id::new(), Id::new(), SystemTime::now(), tree.pid(), ProcessStartTime::of(tree.pid()).unwrap());
        assert_eq!(own.orphans().len(), 1);
        tree.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[cfg(unix)]
    async fn a_reused_pid_with_a_different_start_time_is_not_an_orphan_and_is_removed() {
        let root = tempfile::tempdir().unwrap();
        // A live process-group leader that is not the recorded one.
        let tree = launch(root.path(), "sleep 1000", quick_stop());
        let real = ProcessStartTime::of(tree.pid()).unwrap();
        let earlier = ProcessStartTime { seconds: real.seconds.wrapping_sub(60), microseconds: 0 };
        RunRegistry::new(root.path()).record(Id::new(), Id::new(), SystemTime::now(), tree.pid(), earlier);

        assert!(RunRegistry::new(root.path()).orphans().is_empty());
        assert_eq!(fs::read_dir(root.path().join("runs")).unwrap().count(), 0);
        tree.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    #[cfg(unix)]
    async fn a_group_whose_leader_exited_is_still_an_orphan() {
        let root = tempfile::tempdir().unwrap();
        // The shell exits; a leftover stays in its group, as if localhost
        // crashed before it could clean up.
        let marker = root.path().join("ready");
        let command_line = format!(
            "(trap '' INT TERM; touch '{marker}'; sleep 1000) & while [ ! -e '{marker}' ]; do sleep 0.01; done; exit 0",
            marker = marker.display()
        );
        let patient =
            StopPolicy { interrupt_timeout: Duration::from_secs(30), terminate_timeout: Duration::from_secs(30) };
        let tree = launch(root.path(), &command_line, patient);
        let run_id = record(root.path(), &tree);
        tree.wait_for_shell_exit().await;
        assert_eq!(ProcessStartTime::of(tree.pid()), None);

        let registry = RunRegistry::new(root.path());
        let orphans = registry.orphans();
        assert_eq!(orphans.iter().map(|o| o.run_id).collect::<Vec<_>>(), [run_id]);

        registry.stop_orphans(&orphans, quick_stop()).await;
        assert!(!process_group_exists(tree.pid()));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[cfg(unix)]
    async fn stopping_orphans_kills_their_whole_trees_in_parallel_and_forgets_them() {
        let root = tempfile::tempdir().unwrap();
        let first = launch(root.path(), "sleep 1000 & sleep 1000 & wait", quick_stop());
        let second = launch(root.path(), "trap '' INT TERM; sleep 1000", quick_stop());
        record(root.path(), &first);
        record(root.path(), &second);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let registry = RunRegistry::new(root.path());
        let orphans = registry.orphans();
        assert_eq!(orphans.len(), 2);

        let started = Instant::now();
        registry.stop_orphans(&orphans, quick_stop()).await;

        for orphan in &orphans {
            assert!(!process_group_exists(orphan.process_group_id));
        }
        assert!(registry.orphans().is_empty());
        // Stopped in parallel: the SIGKILL-needing one sets the pace.
        assert!(started.elapsed() < Duration::from_millis(1500));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[cfg(unix)]
    async fn leaving_orphans_forgets_them_without_touching_their_processes() {
        let root = tempfile::tempdir().unwrap();
        let tree = launch(root.path(), "sleep 1000", quick_stop());
        record(root.path(), &tree);
        let registry = RunRegistry::new(root.path());
        let orphans = registry.orphans();

        registry.leave_orphans(&orphans);

        assert_eq!(fs::read_dir(root.path().join("runs")).unwrap().count(), 0);
        assert!(RunRegistry::new(root.path()).orphans().is_empty());
        assert!(process_group_exists(tree.pid()));
        tree.stop().await;
    }

    #[test]
    fn a_damaged_record_is_removed() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("runs")).unwrap();
        fs::write(root.path().join("runs/garbage.json"), "{").unwrap();
        assert!(RunRegistry::new(root.path()).orphans().is_empty());
        assert_eq!(fs::read_dir(root.path().join("runs")).unwrap().count(), 0);
    }

    #[test]
    fn describes_an_orphan_by_its_service() {
        let projects = [Project::with_services("My SaaS", &[("API", None)])];
        let orphan = |service_id| Orphan {
            run_id: Id::new(),
            service_id,
            process_group_id: 4242,
            started_at: SystemTime::now(),
            start_time: ProcessStartTime { seconds: 0, microseconds: 0 },
        };
        assert_eq!(orphan(projects[0].services[0].id).description(&projects), "My SaaS › API");
        assert_eq!(orphan(Id::new()).description(&projects), "A removed service (process group 4242)");
    }
}
