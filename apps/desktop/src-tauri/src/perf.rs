//! A measurement mode for the log view (`apps/desktop/scripts/log-perf.sh`).
//! With `LOCALHOST_PERF=1` the app opens a throwaway storage folder with one
//! Project of very chatty Services; the UI starts them all, shows the
//! combined stream, measures its frames and reports here, and the app quits.
//!
//! `LOCALHOST_PERF_SERVICES` (default 4), `LOCALHOST_PERF_RATE` (lines per
//! second per Service, default 1000) and `LOCALHOST_PERF_SECONDS` (default 30)
//! tune it.

use std::path::PathBuf;

use localhost_core::{Key, LocalhostError, NewService, Origin};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::{AppState, Core};

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PerfSettings {
    services: u32,
    lines_per_second: u32,
    seconds: u32,
    #[serde(skip)]
    root: PathBuf,
}

impl PerfSettings {
    pub fn from_environment() -> Option<PerfSettings> {
        std::env::var_os("LOCALHOST_PERF")?;
        let number = |name: &str, default: u32| {
            std::env::var(name).ok().and_then(|value| value.parse().ok()).filter(|&n| n > 0).unwrap_or(default)
        };
        Some(PerfSettings {
            services: number("LOCALHOST_PERF_SERVICES", 4),
            lines_per_second: number("LOCALHOST_PERF_RATE", 1000),
            seconds: number("LOCALHOST_PERF_SECONDS", 30),
            root: std::env::temp_dir().join(format!("localhost-perf-{}", std::process::id())),
        })
    }

    /// Prints a coloured, request-log-like line `lines_per_second` times a
    /// second, in batches every 0.1s, with nothing but shell builtins.
    fn command_line(&self) -> String {
        let batch = (self.lines_per_second / 10).max(1);
        format!(
            "i=0; while :; do n=0; while [ $n -lt {batch} ]; do i=$((i+1)); n=$((n+1)); \
             printf '\\033[2m%06d\\033[0m \\033[32mINFO\\033[0m GET /api/items/%d 200 \\033[36m%dms\\033[0m\\n' \
             \"$i\" \"$i\" \"$((i % 97))\"; done; sleep 0.1; done"
        )
    }
}

/// The core over a fresh folder holding just the chatty Project.
pub fn open_core(settings: &PerfSettings) -> Result<Core, LocalhostError> {
    let _ = std::fs::remove_dir_all(&settings.root);
    std::fs::create_dir_all(&settings.root).map_err(|error| LocalhostError::PersistenceFailed {
        path: settings.root.display().to_string(),
        reason: error.to_string(),
    })?;
    let core = Core::open(&settings.root)?;
    let services = (1..=settings.services)
        .map(|index| {
            let key = Key::new(format!("chatty-{index}")).expect("a valid Key");
            NewService::new(format!("Chatty {index}"), key, settings.command_line())
        })
        .collect();
    let path = settings.root.to_string_lossy();
    core.library.add_project(
        "Log performance",
        Key::new("log-perf").expect("a valid Key"),
        &path,
        services,
        Origin::Cli,
    )?;
    Ok(core)
}

/// The run to measure, if this is a measurement.
#[tauri::command]
#[specta::specta]
pub fn perf_settings(state: tauri::State<'_, AppState>) -> Option<PerfSettings> {
    state.perf.clone()
}

/// What the UI measured.
#[derive(Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PerfReport {
    seconds: f64,
    lines_received: u32,
    frames: u32,
    frame_ms_p50: f64,
    frame_ms_p95: f64,
    frame_ms_max: f64,
    /// Frames that took longer than two at 60 Hz (33 ms).
    long_frames: u32,
}

/// Prints the report as one JSON line and quits, which stops the Services.
#[tauri::command]
#[specta::specta]
pub fn finish_perf_run(app: AppHandle, state: tauri::State<'_, AppState>, report: PerfReport) {
    let settings = state.perf.as_ref().map(|settings| serde_json::to_value(settings).unwrap_or_default());
    let line = serde_json::json!({ "settings": settings, "report": report });
    println!("LOCALHOST_PERF_REPORT {line}");
    app.exit(0);
}

/// Deletes the throwaway folder, once its Services have stopped.
pub fn clean_up(settings: &PerfSettings) {
    let _ = std::fs::remove_dir_all(&settings.root);
}
