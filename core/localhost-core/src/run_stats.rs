//! What a Run's process tree uses: its CPU, memory and processes, for a
//! quick look at which Service is busy (ADR-0012).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::Id;
use crate::process::TreeUsage;

/// What a Run's process tree used at one moment. CPU use needs two samples,
/// so each call to `ServiceManager::run_stats` is given the previous one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RunStats {
    pub run_id: Id,
    /// The live processes in the Run's tree.
    pub process_count: u32,
    /// Their memory, counting pages they share once: the physical footprint
    /// on macOS (Activity Monitor's "Memory"), the proportional set size on
    /// Linux, private bytes on Windows.
    pub memory_bytes: u64,
    /// CPU time the tree has used, in microseconds.
    pub cpu_time_microseconds: u64,
    /// When the sample was taken, in microseconds on the sampling core's
    /// monotonic clock. Only for comparing samples.
    pub sampled_at_microseconds: u64,
    /// CPU use since the previous sample, where 100 is one core fully busy.
    /// `None` without a previous sample of the same Run.
    pub cpu_percent: Option<f64>,
}

impl RunStats {
    pub(crate) fn new(run_id: Id, usage: TreeUsage, sampled_at: Duration, previous: Option<&RunStats>) -> RunStats {
        let cpu_time_microseconds = usage.cpu_time.as_micros() as u64;
        let sampled_at_microseconds = sampled_at.as_micros() as u64;
        let cpu_percent = previous
            .filter(|previous| previous.run_id == run_id && previous.sampled_at_microseconds < sampled_at_microseconds)
            .map(|previous| {
                // Time of a process that ended without being waited for in
                // the tree is gone from the sum: never below zero.
                let used = cpu_time_microseconds.saturating_sub(previous.cpu_time_microseconds);
                used as f64 * 100.0 / (sampled_at_microseconds - previous.sampled_at_microseconds) as f64
            });
        RunStats {
            run_id,
            process_count: usage.process_count,
            memory_bytes: usage.memory_bytes,
            cpu_time_microseconds,
            sampled_at_microseconds,
            cpu_percent,
        }
    }
}

/// Time on a monotonic clock that started with the first sample.
pub(crate) fn sampling_clock() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(cpu_milliseconds: u64) -> TreeUsage {
        TreeUsage { process_count: 3, cpu_time: Duration::from_millis(cpu_milliseconds), memory_bytes: 4096 }
    }

    #[test]
    fn cpu_use_is_the_cpu_time_used_between_two_samples() {
        let run = Id::new();
        let first = RunStats::new(run, usage(1_000), Duration::from_secs(10), None);
        assert_eq!(first.cpu_percent, None);
        assert_eq!((first.process_count, first.memory_bytes, first.cpu_time_microseconds), (3, 4096, 1_000_000));

        // 3 s of CPU time in 2 s: one and a half cores.
        let second = RunStats::new(run, usage(4_000), Duration::from_secs(12), Some(&first));
        assert_eq!(second.cpu_percent, Some(150.0));

        // A process ended and took its time with it.
        let third = RunStats::new(run, usage(3_000), Duration::from_secs(13), Some(&second));
        assert_eq!(third.cpu_percent, Some(0.0));
    }

    #[test]
    fn a_sample_of_another_run_or_not_earlier_gives_no_cpu_use() {
        let first = RunStats::new(Id::new(), usage(0), Duration::from_secs(10), None);
        assert_eq!(RunStats::new(Id::new(), usage(500), Duration::from_secs(11), Some(&first)).cpu_percent, None);
        assert_eq!(RunStats::new(first.run_id, usage(500), Duration::from_secs(10), Some(&first)).cpu_percent, None);
    }
}
