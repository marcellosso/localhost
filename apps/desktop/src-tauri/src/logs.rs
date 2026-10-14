//! Log output for the UI. Lines never cross one at a time: they are parsed
//! for ANSI styles here and sent in batches at most once per frame
//! (`docs/cross-platform.md`, Tauri UI notes). The buffers stay in the core.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use localhost_core::{Ansi, Id, LocalUrl, OutputChannel, OutputLine, Span};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::Channel;
use tokio::sync::mpsc;

use crate::AppState;
use crate::bridge::CommandResult;

/// How often batches are sent: about one frame.
const FLUSH_INTERVAL: Duration = Duration::from_millis(16);

/// The most lines a log keeps, as the core's buffer of one Run. A burst
/// larger than that between two flushes keeps only its newest lines.
pub const MAX_LINES: usize = 10_000;

/// What a log shows.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LogSource {
    /// One Run's output: a Service's log.
    #[serde(rename_all = "camelCase")]
    Run { run_id: Id },
    /// The latest Runs of these Services, and any they start later, in one stream.
    #[serde(rename_all = "camelCase")]
    Services { service_ids: Vec<Id> },
}

/// A line ready to draw.
#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// Which Service it came from, in a combined stream.
    service_id: Option<Id>,
    channel: OutputChannel,
    spans: Vec<Span>,
    /// The local URLs in the line's text, to open on click.
    links: Vec<LogLink>,
}

/// A local URL in a line: UTF-16 offsets into its text (the spans joined),
/// as JavaScript counts, and the URL to open (`0.0.0.0` as `localhost`).
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct LogLink {
    start: u32,
    end: u32,
    url: String,
}

impl LogLine {
    fn new(line: &OutputLine, service_id: Option<Id>) -> LogLine {
        let spans = Ansi::parse(&line.text).spans;
        let links = if line.channel == OutputChannel::Localhost { Vec::new() } else { links(&spans) };
        LogLine { service_id, channel: line.channel, spans, links }
    }
}

fn links(spans: &[Span]) -> Vec<LogLink> {
    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
    if !text.contains("http") {
        return Vec::new();
    }
    let utf16 = |byte: usize| text[..byte].encode_utf16().count() as u32;
    LocalUrl::matches(&text)
        .into_iter()
        .map(|(range, url)| LogLink { start: utf16(range.start), end: utf16(range.end), url })
        .collect()
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogBatch {
    lines: Vec<LogLine>,
    /// Lines that arrived but were left out because the batch kept only its
    /// newest `MAX_LINES`.
    skipped: u32,
    /// No more lines will come: the Run's output ended.
    is_finished: bool,
}

/// The open streams, so a closed log stops its task.
#[derive(Default)]
pub struct LogStreams {
    next_id: AtomicU32,
    tasks: Mutex<HashMap<u32, JoinHandle<()>>>,
}

/// Streams `source` over `batches`: what is buffered first, then new
/// lines. Returns the id to close it with.
#[tauri::command]
#[specta::specta]
pub fn open_log(
    state: tauri::State<'_, AppState>,
    source: LogSource,
    batches: Channel<LogBatch>,
) -> CommandResult<u32> {
    let logs = &state.core()?.logs;
    let (buffered, receiver) = match &source {
        LogSource::Run { run_id } => {
            let (lines, receiver) = logs.subscribe_to_run(*run_id);
            let lines: Vec<LogLine> = lines.iter().map(|line| LogLine::new(line, None)).collect();
            let (sender, combined) = mpsc::unbounded_channel();
            forward(receiver, sender, |line| LogLine::new(&line, None));
            (lines, combined)
        }
        LogSource::Services { service_ids } => {
            let ids: HashSet<Id> = service_ids.iter().copied().collect();
            let (lines, receiver) = logs.subscribe_to_services(ids);
            let lines = lines.iter().map(|line| LogLine::new(&line.line, Some(line.service_id))).collect();
            let (sender, combined) = mpsc::unbounded_channel();
            forward(receiver, sender, |line| LogLine::new(&line.line, Some(line.service_id)));
            (lines, combined)
        }
    };
    let id = state.logs.next_id.fetch_add(1, Ordering::Relaxed);
    let task = tauri::async_runtime::spawn(stream(buffered, receiver, batches));
    state.logs.tasks.lock().unwrap().insert(id, task);
    Ok(id)
}

/// Clears the Services' buffers (the log files are kept). Open logs of them
/// show only what comes next once reopened.
#[tauri::command]
#[specta::specta]
pub fn clear_logs(state: tauri::State<'_, AppState>, service_ids: Vec<Id>) -> CommandResult {
    let logs = &state.core()?.logs;
    for service_id in service_ids {
        logs.clear_buffer(service_id);
    }
    Ok(())
}

impl LogStreams {
    /// Stops every stream, once the window is gone.
    pub fn close_all(&self) {
        for (_, task) in self.tasks.lock().unwrap().drain() {
            task.abort();
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn close_log(state: tauri::State<'_, AppState>, id: u32) {
    if let Some(task) = state.logs.tasks.lock().unwrap().remove(&id) {
        task.abort();
    }
}

/// Converts lines on the way, so `stream` handles both sources alike. Ends
/// when either side does.
fn forward<T: Send + 'static>(
    mut receiver: mpsc::UnboundedReceiver<T>,
    sender: mpsc::UnboundedSender<LogLine>,
    convert: impl Fn(T) -> LogLine + Send + 'static,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(line) = receiver.recv().await {
            if sender.send(convert(line)).is_err() {
                return;
            }
        }
    });
}

async fn stream(buffered: Vec<LogLine>, mut receiver: mpsc::UnboundedReceiver<LogLine>, batches: Channel<LogBatch>) {
    let mut pending = Batch::default();
    for line in buffered {
        pending.push(line);
    }
    let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
    loop {
        tokio::select! {
            line = receiver.recv() => match line {
                Some(line) => pending.push(line),
                None => {
                    let _ = batches.send(pending.take(true));
                    return;
                }
            },
            _ = ticker.tick() => {
                // A send fails once the window is gone.
                if !pending.is_empty() && batches.send(pending.take(false)).is_err() {
                    return;
                }
            }
        }
    }
}

/// Lines waiting for the next flush, at most `MAX_LINES`.
#[derive(Default)]
struct Batch {
    lines: VecDeque<LogLine>,
    skipped: u32,
}

impl Batch {
    fn push(&mut self, line: LogLine) {
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
            self.skipped += 1;
        }
        self.lines.push_back(line);
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.skipped == 0
    }

    fn take(&mut self, is_finished: bool) -> LogBatch {
        let batch = std::mem::take(self);
        LogBatch { lines: batch.lines.into(), skipped: batch.skipped, is_finished }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> LogLine {
        LogLine::new(&OutputLine::new(OutputChannel::Stdout, text), None)
    }

    #[test]
    fn parses_ansi_styles_into_spans() {
        let line = line("\u{1b}[32mready\u{1b}[0m in 3ms");
        let texts: Vec<&str> = line.spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, ["ready", " in 3ms"]);
        assert!(line.spans[0].style.foreground.is_some());
    }

    #[test]
    fn finds_local_urls_by_their_utf16_offsets() {
        let line = line("→ Local: \u{1b}[36mhttp://0.0.0.0:5173/\u{1b}[0m, docs https://example.com");
        assert_eq!(line.links, [LogLink { start: 9, end: 29, url: "http://localhost:5173/".into() }]);
        let note = LogLine::new(&OutputLine::new(OutputChannel::Localhost, "Open http://localhost:1"), None);
        assert!(note.links.is_empty());
    }

    #[test]
    fn a_batch_keeps_only_the_newest_lines() {
        let mut batch = Batch::default();
        for index in 0..MAX_LINES + 5 {
            batch.push(line(&index.to_string()));
        }
        let taken = batch.take(false);
        assert_eq!(taken.lines.len(), MAX_LINES);
        assert_eq!(taken.skipped, 5);
        assert_eq!(taken.lines[0].spans[0].text, "5");
        assert!(batch.is_empty());
    }
}
