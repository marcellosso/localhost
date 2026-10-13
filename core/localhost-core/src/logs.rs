//! Run output: a bounded in-memory buffer of each Service's latest Run for
//! the viewer, and a file per Run at `<root>/logs/<SERVICE-ID>/<RUN-ID>.log`.
//!
//! Each file line is `<ISO 8601 timestamp> <stdout|stderr|localhost> <text>`,
//! with the text as written (ANSI sequences included), exactly as the Swift
//! core wrote it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;

use crate::Id;
use crate::output::{OutputChannel, OutputLine, ServiceOutputLine};

/// A bounded, in-memory buffer of a Run's most recent output lines.
#[derive(Clone, Debug)]
pub struct LogBuffer {
    capacity: usize,
    lines: VecDeque<OutputLine>,
    dropped_count: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> LogBuffer {
        assert!(capacity > 0, "LogBuffer capacity must be positive");
        LogBuffer { capacity, lines: VecDeque::with_capacity(capacity.min(1024)), dropped_count: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// How many lines were dropped to stay within `capacity`.
    pub fn dropped_count(&self) -> usize {
        self.dropped_count
    }

    /// All buffered lines, oldest first.
    pub fn lines(&self) -> Vec<OutputLine> {
        self.lines.iter().cloned().collect()
    }

    /// The last `limit` lines, oldest first.
    pub fn recent_lines(&self, limit: usize) -> Vec<OutputLine> {
        self.lines.iter().skip(self.lines.len().saturating_sub(limit)).cloned().collect()
    }

    /// Empties the buffer; `dropped_count` keeps counting.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn push(&mut self, line: OutputLine) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.dropped_count += 1;
        }
        self.lines.push_back(line);
    }
}

/// Keeps Run output. The last `retained_runs_per_service` files are kept per
/// Service; a file rotates into a `.log.1` backup so the pair stays within
/// `max_file_size`.
pub struct LogStore {
    logs_directory: PathBuf,
    max_lines_per_run: usize,
    max_file_size: usize,
    retained_runs_per_service: usize,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    buffers: HashMap<Id, LogBuffer>,
    latest_run_of_service: HashMap<Id, Id>,
    files: HashMap<Id, LogFile>,
    file_paths: HashMap<Id, PathBuf>,
    service_of_run: HashMap<Id, Id>,
    run_subscribers: HashMap<Id, Vec<mpsc::UnboundedSender<OutputLine>>>,
    service_subscribers: Vec<ServiceSubscriber>,
}

struct ServiceSubscriber {
    service_ids: HashSet<Id>,
    sender: mpsc::UnboundedSender<ServiceOutputLine>,
}

impl LogStore {
    pub const DEFAULT_MAX_LINES_PER_RUN: usize = 10_000;
    pub const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;
    pub const DEFAULT_RETAINED_RUNS_PER_SERVICE: usize = 5;

    pub fn new(root: &Path) -> LogStore {
        LogStore::with_limits(
            root,
            Self::DEFAULT_MAX_LINES_PER_RUN,
            Self::DEFAULT_MAX_FILE_SIZE,
            Self::DEFAULT_RETAINED_RUNS_PER_SERVICE,
        )
    }

    pub fn with_limits(
        root: &Path,
        max_lines_per_run: usize,
        max_file_size: usize,
        retained_runs_per_service: usize,
    ) -> LogStore {
        LogStore {
            logs_directory: root.join("logs"),
            max_lines_per_run,
            max_file_size,
            retained_runs_per_service,
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // MARK: Reading

    /// The buffered lines of a Run, oldest first. Empty unless it is its
    /// Service's latest Run.
    pub fn lines(&self, run_id: Id) -> Vec<OutputLine> {
        self.state().buffers.get(&run_id).map(LogBuffer::lines).unwrap_or_default()
    }

    /// The last `limit` buffered lines of a Run.
    pub fn recent_lines(&self, run_id: Id, limit: usize) -> Vec<OutputLine> {
        self.state().buffers.get(&run_id).map(|buffer| buffer.recent_lines(limit)).unwrap_or_default()
    }

    /// The last `limit` buffered stdout and stderr lines of a Run, leaving
    /// out what localhost wrote, e.g. for a Failed Service's summary.
    pub fn recent_output(&self, run_id: Id, limit: usize) -> Vec<OutputLine> {
        let lines: Vec<OutputLine> =
            self.lines(run_id).into_iter().filter(|line| line.channel != OutputChannel::Localhost).collect();
        lines[lines.len().saturating_sub(limit)..].to_vec()
    }

    pub fn latest_run_id(&self, service_id: Id) -> Option<Id> {
        self.state().latest_run_of_service.get(&service_id).copied()
    }

    /// The buffered lines of a Run plus a stream of every line appended after
    /// them. The stream ends when the Run's output ends.
    pub fn subscribe_to_run(&self, run_id: Id) -> (Vec<OutputLine>, mpsc::UnboundedReceiver<OutputLine>) {
        let mut state = self.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        // An ended Run's stream ends at once: its sender is dropped here.
        if state.files.contains_key(&run_id) {
            state.run_subscribers.entry(run_id).or_default().push(sender);
        }
        let lines = state.buffers.get(&run_id).map(LogBuffer::lines).unwrap_or_default();
        (lines, receiver)
    }

    /// The combined stream of several Services: the buffered lines of each
    /// one's latest Run merged by time, plus a stream of every line appended
    /// to any of their Runs after that, including Runs that begin later. The
    /// stream ends when the subscriber drops it.
    pub fn subscribe_to_services(
        &self,
        service_ids: HashSet<Id>,
    ) -> (Vec<ServiceOutputLine>, mpsc::UnboundedReceiver<ServiceOutputLine>) {
        let mut state = self.state();
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut lines: Vec<ServiceOutputLine> = Vec::new();
        for &service_id in &service_ids {
            let Some(run_id) = state.latest_run_of_service.get(&service_id) else { continue };
            let buffered = state.buffers.get(run_id).map(LogBuffer::lines).unwrap_or_default();
            lines.extend(buffered.into_iter().map(|line| ServiceOutputLine { service_id, line }));
        }
        // Stable, so each Service's lines keep their order.
        lines.sort_by_key(|line| line.line.timestamp);
        state.service_subscribers.push(ServiceSubscriber { service_ids, sender });
        (lines, receiver)
    }

    /// Empties the in-memory buffer of the Service's latest Run, so viewers
    /// start from new output. Its log file is untouched.
    pub fn clear_buffer(&self, service_id: Id) {
        let mut state = self.state();
        if let Some(run_id) = state.latest_run_of_service.get(&service_id).copied()
            && let Some(buffer) = state.buffers.get_mut(&run_id)
        {
            buffer.clear();
        }
    }

    pub fn log_file_path(&self, run_id: Id) -> Option<PathBuf> {
        self.state().file_paths.get(&run_id).cloned()
    }

    /// Deletes a removed Service's log files and buffer. Its Run must have
    /// ended (removing a Service stops it first).
    pub fn remove_logs(&self, service_id: Id) {
        let mut state = self.state();
        if let Some(run_id) = state.latest_run_of_service.remove(&service_id) {
            state.buffers.remove(&run_id);
            state.file_paths.remove(&run_id);
        }
        let _ = fs::remove_dir_all(self.logs_directory.join(service_id.to_string()));
    }

    /// The lines of the Service's latest Run file, oldest first, read back
    /// from disk with its rotated backup: what any session, this one or
    /// another (the app, the CLI), last logged for it. Lines that don't parse,
    /// such as one cut by the size cap, are skipped.
    pub fn stored_lines(&self, service_id: Id) -> Vec<OutputLine> {
        let directory = self.logs_directory.join(service_id.to_string());
        let Some(latest) = run_files_newest_first(&directory).into_iter().next() else { return Vec::new() };
        [backup_path(&latest), latest]
            .iter()
            .filter_map(|path| fs::read(path).ok())
            .flat_map(|data| String::from_utf8_lossy(&data).lines().filter_map(parse_line).collect::<Vec<_>>())
            .collect()
    }
}

// Writing, by the ServiceManager.
impl LogStore {
    pub(crate) fn begin_run(&self, run_id: Id, service_id: Id) {
        let mut state = self.state();
        if let Some(previous) = state.latest_run_of_service.get(&service_id).copied()
            && !state.files.contains_key(&previous)
        {
            state.buffers.remove(&previous);
        }
        state.latest_run_of_service.insert(service_id, run_id);
        state.service_of_run.insert(run_id, service_id);
        state.buffers.insert(run_id, LogBuffer::new(self.max_lines_per_run));

        let directory = self.logs_directory.join(service_id.to_string());
        let path = directory.join(format!("{run_id}.log"));
        state.file_paths.insert(run_id, path.clone());
        state.files.insert(run_id, LogFile::open(path, self.max_file_size));
        self.remove_old_runs(&directory);
    }

    pub(crate) fn append(&self, lines: &[OutputLine], run_id: Id) {
        let mut state = self.state();
        let State { buffers, files, service_of_run, run_subscribers, service_subscribers, .. } = &mut *state;
        let (Some(file), Some(&service_id)) = (files.get_mut(&run_id), service_of_run.get(&run_id)) else { return };
        let mut buffer = buffers.get_mut(&run_id);
        for line in lines {
            if let Some(buffer) = buffer.as_deref_mut() {
                buffer.push(line.clone());
            }
            file.write(line);
            if let Some(subscribers) = run_subscribers.get_mut(&run_id) {
                subscribers.retain(|subscriber| subscriber.send(line.clone()).is_ok());
            }
            service_subscribers.retain(|subscriber| {
                !subscriber.service_ids.contains(&service_id)
                    || subscriber.sender.send(ServiceOutputLine { service_id, line: line.clone() }).is_ok()
            });
        }
    }

    pub(crate) fn end_run(&self, run_id: Id) {
        let mut state = self.state();
        state.files.remove(&run_id);
        // Dropping the senders ends the Run's streams.
        state.run_subscribers.remove(&run_id);
        state.service_of_run.remove(&run_id);
    }

    /// Deletes all but the newest `retained_runs_per_service` Run files.
    fn remove_old_runs(&self, directory: &Path) {
        for path in run_files_newest_first(directory).into_iter().skip(self.retained_runs_per_service) {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(backup_path(&path));
        }
    }
}

/// A Service's Run files (not their backups), the newest first.
fn run_files_newest_first(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else { return Vec::new() };
    let mut runs: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .map(|path| {
            let metadata = fs::metadata(&path).ok();
            // Not every file system records creation; the first write is close enough.
            let created = metadata.and_then(|m| m.created().or_else(|_| m.modified()).ok()).unwrap_or(UNIX_EPOCH);
            (path, created)
        })
        .collect();
    runs.sort_by(|a, b| b.1.cmp(&a.1));
    runs.into_iter().map(|(path, _)| path).collect()
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".1");
    PathBuf::from(backup)
}

/// One Run's log file. Rotates into `<name>.1` when it reaches half of
/// `max_size`, so the file and its backup never exceed `max_size` together.
struct LogFile {
    path: PathBuf,
    segment_size: usize,
    file: Option<fs::File>,
    size: usize,
}

impl LogFile {
    fn open(path: PathBuf, max_size: usize) -> LogFile {
        if let Some(directory) = path.parent() {
            let _ = fs::create_dir_all(directory);
        }
        let mut file = LogFile { path, segment_size: max_size / 2, file: None, size: 0 };
        file.reopen();
        file
    }

    fn write(&mut self, line: &OutputLine) {
        let text = format!("{} {} {}\n", iso8601(line.timestamp), line.channel.as_str(), line.text);
        let mut data = text.as_bytes();
        if data.len() > self.segment_size {
            data = &data[data.len() - self.segment_size..];
        }
        if self.size + data.len() > self.segment_size {
            self.rotate();
        }
        if let Some(file) = &mut self.file {
            let _ = file.write_all(data);
        }
        self.size += data.len();
    }

    fn reopen(&mut self) {
        self.file = fs::File::create(&self.path).ok();
        self.size = 0;
    }

    fn rotate(&mut self) {
        self.file = None;
        let backup = backup_path(&self.path);
        let _ = fs::remove_file(&backup);
        let _ = fs::rename(&self.path, &backup);
        self.reopen();
    }
}

/// `2027-01-15T08:00:00.250Z`: UTC, with milliseconds.
pub(crate) fn iso8601(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since_epoch.as_secs() as i64;
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60,
        since_epoch.subsec_millis()
    )
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The day count since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_index = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let day_of_year = (153 * month_index + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Reads back what `iso8601` writes, and nothing else.
fn parse_iso8601(text: &str) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    let shape = "0000-00-00T00:00:00.000Z".as_bytes();
    let matches_shape = bytes.len() == shape.len()
        && bytes.iter().zip(shape).all(|(byte, expected)| match expected {
            b'0' => byte.is_ascii_digit(),
            _ => byte == expected,
        });
    if !matches_shape {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().ok();
    let (year, month, day) = (i64::from(number(0..4)?), number(5..7)?, number(8..10)?);
    let (hour, minute, second, millis) = (number(11..13)?, number(14..16)?, number(17..19)?, number(20..23)?);
    let days = days_from_civil(year, month, day);
    if civil_from_days(days) != (year, month, day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let seconds = u64::try_from(days * 86_400).ok()? + u64::from(hour * 3600 + minute * 60 + second);
    Some(UNIX_EPOCH + std::time::Duration::from_secs(seconds) + std::time::Duration::from_millis(millis.into()))
}

/// A log file line, `<timestamp> <channel> <text>`, as `LogFile` writes it.
fn parse_line(line: &str) -> Option<OutputLine> {
    let (timestamp, rest) = line.split_once(' ')?;
    let (channel, text) = rest.split_once(' ')?;
    let channel = [OutputChannel::Stdout, OutputChannel::Stderr, OutputChannel::Localhost]
        .into_iter()
        .find(|candidate| candidate.as_str() == channel)?;
    Some(OutputLine { channel, text: text.into(), timestamp: parse_iso8601(timestamp)? })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn line(text: &str) -> OutputLine {
        on(OutputChannel::Stdout, text)
    }

    fn on(channel: OutputChannel, text: &str) -> OutputLine {
        OutputLine { channel, text: text.into(), timestamp: UNIX_EPOCH + Duration::from_millis(1_800_000_000_250) }
    }

    fn at(seconds: u64, channel: OutputChannel, text: &str) -> OutputLine {
        OutputLine { channel, text: text.into(), timestamp: UNIX_EPOCH + Duration::from_secs(seconds) }
    }

    fn texts(lines: &[OutputLine]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    struct Setup {
        root: tempfile::TempDir,
    }

    impl Setup {
        fn new() -> Setup {
            Setup { root: tempfile::tempdir().unwrap() }
        }

        fn store(&self) -> LogStore {
            LogStore::new(self.root.path())
        }

        fn log_directory(&self, service_id: Id) -> PathBuf {
            self.root.path().join("logs").join(service_id.to_string())
        }
    }

    /// Begins a Run of a new Service (or of `service_id`), returning (run, service).
    fn begin(store: &LogStore, service_id: Option<Id>) -> (Id, Id) {
        let (run_id, service_id) = (Id::new(), service_id.unwrap_or_default());
        store.begin_run(run_id, service_id);
        (run_id, service_id)
    }

    // MARK: LogBuffer

    #[test]
    fn a_buffer_keeps_lines_in_order_until_full() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(line("a"));
        buffer.push(line("b"));
        assert_eq!(texts(&buffer.lines()), ["a", "b"]);
        assert_eq!(buffer.dropped_count(), 0);
    }

    #[test]
    fn a_buffer_drops_the_oldest_lines_once_full() {
        let mut buffer = LogBuffer::new(3);
        for text in ["a", "b", "c", "d", "e"] {
            buffer.push(line(text));
        }
        assert_eq!(texts(&buffer.lines()), ["c", "d", "e"]);
        assert_eq!(buffer.dropped_count(), 2);
    }

    #[test]
    fn recent_lines_are_bounded_by_the_limit() {
        let mut buffer = LogBuffer::new(10);
        for text in ["a", "b", "c", "d"] {
            buffer.push(line(text));
        }
        assert_eq!(texts(&buffer.recent_lines(2)), ["c", "d"]);
        assert_eq!(texts(&buffer.recent_lines(100)), ["a", "b", "c", "d"]);
    }

    // MARK: LogStore

    #[test]
    fn writes_each_run_to_logs_service_run_id_dot_log() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, service) = begin(&store, None);
        store.append(&[line("hello")], run);
        store.append(&[on(OutputChannel::Stderr, "oops")], run);
        store.append(&[on(OutputChannel::Localhost, "Exited with code 0")], run);
        store.end_run(run);

        let file = setup.log_directory(service).join(format!("{run}.log"));
        assert_eq!(store.log_file_path(run), Some(file.clone()));
        assert_eq!(
            fs::read_to_string(file).unwrap(),
            "2027-01-15T08:00:00.250Z stdout hello\n\
             2027-01-15T08:00:00.250Z stderr oops\n\
             2027-01-15T08:00:00.250Z localhost Exited with code 0\n"
        );
    }

    #[test]
    fn keeps_lines_in_memory_with_their_channel_and_timestamp() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, _) = begin(&store, None);
        store.append(&[line("a"), on(OutputChannel::Stderr, "b")], run);
        assert_eq!(store.lines(run), [line("a"), on(OutputChannel::Stderr, "b")]);
    }

    #[test]
    fn the_in_memory_buffer_is_bounded() {
        let setup = Setup::new();
        let store = LogStore::with_limits(setup.root.path(), 2, LogStore::DEFAULT_MAX_FILE_SIZE, 5);
        let (run, _) = begin(&store, None);
        for text in ["a", "b", "c"] {
            store.append(&[line(text)], run);
        }

        assert_eq!(texts(&store.lines(run)), ["b", "c"]);
        // The file keeps everything.
        let contents = fs::read_to_string(store.log_file_path(run).unwrap()).unwrap();
        assert_eq!(contents.lines().count(), 3);
    }

    #[test]
    fn keeps_only_the_latest_run_of_each_service_in_memory() {
        let setup = Setup::new();
        let store = setup.store();
        let (first, service) = begin(&store, None);
        store.append(&[line("first")], first);
        store.end_run(first);

        let (second, _) = begin(&store, Some(service));

        assert!(store.lines(first).is_empty());
        assert!(store.lines(second).is_empty());
        assert_eq!(store.latest_run_id(service), Some(second));
    }

    #[test]
    fn keeps_the_files_of_the_last_5_runs_per_service() {
        let setup = Setup::new();
        let store = setup.store();
        let service = Id::new();
        let mut runs = Vec::new();
        for index in 0..7 {
            let (run, _) = begin(&store, Some(service));
            runs.push(run);
            store.append(&[line(&format!("run {index}"))], run);
            store.end_run(run);
            // Creation times must differ, even on file systems that record them coarsely.
            std::thread::sleep(Duration::from_millis(10));
        }

        let mut remaining: Vec<String> = fs::read_dir(setup.log_directory(service))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        remaining.sort();
        let mut expected: Vec<String> = runs[2..].iter().map(|run| format!("{run}.log")).collect();
        expected.sort();
        assert_eq!(remaining, expected);
    }

    #[test]
    fn removing_a_services_logs_deletes_its_files_and_buffer_and_no_other_services() {
        let setup = Setup::new();
        let store = setup.store();
        let (removed, removed_service) = begin(&store, None);
        let (kept, kept_service) = begin(&store, None);
        for run in [removed, kept] {
            store.append(&[line("output")], run);
            store.end_run(run);
        }

        store.remove_logs(removed_service);

        assert!(!setup.log_directory(removed_service).exists());
        assert!(store.lines(removed).is_empty());
        assert_eq!(store.latest_run_id(removed_service), None);
        assert!(setup.log_directory(kept_service).exists());
        assert_eq!(texts(&store.lines(kept)), ["output"]);
    }

    #[test]
    fn rotates_a_runs_file_so_it_stays_within_the_size_cap() {
        let setup = Setup::new();
        let store = LogStore::with_limits(setup.root.path(), 10_000, 1_000, 5);
        let (run, _) = begin(&store, None);
        for index in 0..100 {
            store.append(&[line(&format!("line {index} {}", "x".repeat(20)))], run);
        }
        store.end_run(run);

        let file = store.log_file_path(run).unwrap();
        let size = |path: &Path| fs::metadata(path).unwrap().len();
        assert!(size(&file) + size(&backup_path(&file)) <= 1_000);
        // The most recent output is kept.
        assert!(fs::read_to_string(&file).unwrap().ends_with(&format!("line 99 {}\n", "x".repeat(20))));
    }

    #[test]
    fn subscribing_returns_the_lines_so_far_then_live_lines_until_the_run_ends() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, _) = begin(&store, None);
        store.append(&[line("before")], run);

        let (lines, mut updates) = store.subscribe_to_run(run);
        store.append(&[line("after")], run);
        store.end_run(run);

        assert_eq!(texts(&lines), ["before"]);
        let mut live = Vec::new();
        while let Ok(line) = updates.try_recv() {
            live.push(line.text);
        }
        assert_eq!(live, ["after"]);
        assert!(updates.is_closed());
    }

    #[test]
    fn the_combined_stream_merges_services_by_time_and_follows_their_next_runs() {
        let setup = Setup::new();
        let store = setup.store();
        let (web_run, web) = begin(&store, None);
        let (api_run, api) = begin(&store, None);
        let (other_run, _) = begin(&store, None);
        store.append(&[at(1, OutputChannel::Stdout, "web 1"), at(3, OutputChannel::Stdout, "web 3")], web_run);
        store.append(&[at(2, OutputChannel::Stderr, "api 2")], api_run);
        store.append(&[at(2, OutputChannel::Stdout, "other")], other_run);

        let (lines, mut updates) = store.subscribe_to_services(HashSet::from([web, api]));
        assert_eq!(lines.iter().map(|l| l.line.text.as_str()).collect::<Vec<_>>(), ["web 1", "api 2", "web 3"]);
        assert_eq!(lines.iter().map(|l| l.service_id).collect::<Vec<_>>(), [web, api, web]);

        store.append(&[line("api live")], api_run);
        store.append(&[line("other live")], other_run);
        store.end_run(web_run);
        let (next_web_run, _) = begin(&store, Some(web));
        store.append(&[line("web again")], next_web_run);

        let live: Vec<ServiceOutputLine> = std::iter::from_fn(|| updates.try_recv().ok()).collect();
        assert_eq!(live.iter().map(|l| l.line.text.as_str()).collect::<Vec<_>>(), ["api live", "web again"]);
        assert_eq!(live.iter().map(|l| l.service_id).collect::<Vec<_>>(), [api, web]);
    }

    #[test]
    fn clearing_a_services_buffer_keeps_its_log_file_and_new_output() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, service) = begin(&store, None);
        store.append(&[line("old")], run);
        store.clear_buffer(service);
        store.append(&[line("new")], run);

        assert_eq!(texts(&store.lines(run)), ["new"]);
        assert_eq!(fs::read_to_string(store.log_file_path(run).unwrap()).unwrap().lines().count(), 2);
    }

    #[test]
    fn recent_output_leaves_out_localhosts_own_lines() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, _) = begin(&store, None);
        for text in ["a", "b", "c"] {
            store.append(&[line(text)], run);
        }
        store.append(&[on(OutputChannel::Stderr, "d"), on(OutputChannel::Localhost, "Exited with code 1")], run);

        assert_eq!(texts(&store.recent_output(run, 3)), ["b", "c", "d"]);
    }

    #[test]
    fn subscribing_to_an_ended_run_returns_its_lines_and_a_finished_stream() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, _) = begin(&store, None);
        store.append(&[line("only")], run);
        store.end_run(run);

        let (lines, mut updates) = store.subscribe_to_run(run);
        assert_eq!(texts(&lines), ["only"]);
        assert!(updates.try_recv().is_err());
        assert!(updates.is_closed());
    }

    #[test]
    fn a_dropped_combined_subscriber_is_forgotten() {
        let setup = Setup::new();
        let store = setup.store();
        let (run, service) = begin(&store, None);
        drop(store.subscribe_to_services(HashSet::from([service])));
        store.append(&[line("a")], run);
        assert!(store.state().service_subscribers.is_empty());
    }

    #[test]
    fn stored_lines_read_back_the_latest_run_from_disk() {
        let setup = Setup::new();
        let store = setup.store();
        let (first, service) = begin(&store, None);
        store.append(&[line("first run")], first);
        store.end_run(first);
        std::thread::sleep(Duration::from_millis(10));
        let (second, _) = begin(&store, Some(service));
        let written = [
            at(1, OutputChannel::Stdout, "with  two spaces"),
            at(2, OutputChannel::Stderr, ""),
            on(OutputChannel::Localhost, "Exited with code 0"),
        ];
        store.append(&written, second);

        // Another session, which never ran the Service, reads what this one wrote.
        assert_eq!(setup.store().stored_lines(service), written);
    }

    #[test]
    fn stored_lines_include_the_rotated_backup_first() {
        let setup = Setup::new();
        let store = LogStore::with_limits(setup.root.path(), 10_000, 1_000, 5);
        let (run, service) = begin(&store, None);
        for index in 0..100 {
            store.append(&[line(&format!("line {index} {}", "x".repeat(20)))], run);
        }
        store.end_run(run);

        let in_file = fs::read_to_string(store.log_file_path(run).unwrap()).unwrap().lines().count();
        let stored = setup.store().stored_lines(service);
        let indexes: Vec<usize> =
            stored.iter().map(|line| line.text.split(' ').nth(1).unwrap().parse().unwrap()).collect();
        assert!(indexes.len() > in_file, "the backup's lines are read too");
        assert_eq!(indexes.last(), Some(&99));
        assert!(indexes.windows(2).all(|pair| pair[1] == pair[0] + 1), "in order: {indexes:?}");
    }

    #[test]
    fn stored_lines_skip_what_does_not_parse() {
        let setup = Setup::new();
        let service = Id::new();
        fs::create_dir_all(setup.log_directory(service)).unwrap();
        fs::write(
            setup.log_directory(service).join(format!("{}.log", Id::new())),
            "x truncated by the size cap\n\
             2027-01-15T08:00:00.250Z stdout kept\n\
             2027-01-15T08:00:00.250Z elsewhere unknown channel\n\
             2027-13-15T08:00:00.250Z stdout bad month\n",
        )
        .unwrap();

        assert_eq!(setup.store().stored_lines(service), [line("kept")]);
    }

    #[test]
    fn a_service_that_never_ran_has_no_stored_lines() {
        assert!(Setup::new().store().stored_lines(Id::new()).is_empty());
    }

    #[test]
    fn parses_the_timestamps_it_writes() {
        for millis in [0, 951_782_400_999, 1_800_000_000_250, 4_102_444_799_000] {
            let time = UNIX_EPOCH + Duration::from_millis(millis);
            assert_eq!(parse_iso8601(&iso8601(time)), Some(time));
        }
        assert_eq!(parse_iso8601("2027-01-15T08:00:00Z"), None);
        assert_eq!(parse_iso8601("2027-02-30T08:00:00.000Z"), None);
    }

    #[test]
    fn formats_timestamps_as_iso8601_utc_with_milliseconds() {
        assert_eq!(iso8601(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(UNIX_EPOCH + Duration::from_millis(951_782_400_999)), "2000-02-29T00:00:00.999Z");
        assert_eq!(iso8601(UNIX_EPOCH + Duration::from_secs(4_102_444_799)), "2099-12-31T23:59:59.000Z");
    }
}
