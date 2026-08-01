//! Machine-readable profiling-consumer self-accounting.
//!
//! The counters live only on the consumer thread, so updating them needs no
//! atomics and cannot perturb producers. `BAML_OBS_STATS` snapshots are
//! written at durability/engine-close barriers, which are also the points
//! benchmark harnesses use to delimit a sample.

#![allow(unsafe_code)]

use std::{
    io,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::prof::config::ProfilePipeline;

#[derive(Debug, Default)]
pub(crate) struct ConsumerCounters {
    pub(crate) raw_bytes_drained: u64,
    pub(crate) drained_ranges: u64,
    pub(crate) events: u64,
    pub(crate) cct_blocks: u64,
    pub(crate) cct_bytes: u64,
    pub(crate) flushes: u64,
    pub(crate) fsyncs: u64,
    pub(crate) writer_failures: u64,
    pub(crate) corrupt_ranges: u64,
    pub(crate) closed_record_ranges: u64,
    pub(crate) shed_recorder: u64,
    pub(crate) shed_full_trace: u64,
    pub(crate) shed_value_encoding: u64,
    pub(crate) shed_structural_ranges: u64,
    pub(crate) dropped_dumps: u64,
}

#[derive(Debug)]
pub(crate) struct ConsumerStats {
    path: Option<PathBuf>,
    pipeline: ProfilePipeline,
    started: Instant,
    started_cpu_ns: Option<u64>,
    pub(crate) counters: ConsumerCounters,
}

pub(crate) struct Snapshot<'a> {
    pub(crate) process_id: &'a str,
    pub(crate) profile_bytes: u64,
    pub(crate) ring_live_bytes: usize,
    pub(crate) ring_peak_bytes: usize,
    pub(crate) reason: &'a str,
}

impl ConsumerStats {
    pub(crate) fn new(path: Option<PathBuf>, pipeline: ProfilePipeline) -> Self {
        Self {
            path,
            pipeline,
            started: Instant::now(),
            started_cpu_ns: thread_cpu_time_ns(),
            counters: ConsumerCounters::default(),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.path.is_some()
    }

    pub(crate) fn write(&self, snapshot: Snapshot<'_>) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let cpu_time_ns = thread_cpu_time_ns().and_then(|now| {
            self.started_cpu_ns
                .map(|started| now.saturating_sub(started))
        });
        let value = serde_json::json!({
            "schema_version": 1,
            "kind": "baml_observability_consumer_stats",
            "pid": std::process::id(),
            "process_id": snapshot.process_id,
            "pipeline": self.pipeline.as_str(),
            "snapshot_reason": snapshot.reason,
            "wall_time_ns": u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            "consumer_cpu_ns": cpu_time_ns,
            "raw_bytes_drained": self.counters.raw_bytes_drained,
            "drained_ranges": self.counters.drained_ranges,
            "events": self.counters.events,
            "profile_bytes": snapshot.profile_bytes,
            "cct_blocks": self.counters.cct_blocks,
            "cct_bytes": self.counters.cct_bytes,
            "flushes": self.counters.flushes,
            "fsyncs": self.counters.fsyncs,
            "writer_failures": self.counters.writer_failures,
            "corrupt_ranges": self.counters.corrupt_ranges,
            "closed_record_ranges": self.counters.closed_record_ranges,
            "dropped_dumps": self.counters.dropped_dumps,
            "ring_live_bytes": snapshot.ring_live_bytes,
            "ring_peak_bytes": snapshot.ring_peak_bytes,
            "shed": {
                "recorder": self.counters.shed_recorder,
                "full_trace": self.counters.shed_full_trace,
                "value_encoding": self.counters.shed_value_encoding,
                "structural_ranges": self.counters.shed_structural_ranges,
            },
        });
        write_json(path, &value)
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    #[cfg(windows)]
    {
        // Windows rename does not replace an existing destination. Stats are
        // diagnostic rather than a durability boundary, so prefer reliable
        // repeated snapshots there.
        return std::fs::write(path, bytes);
    }
    #[cfg(not(windows))]
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    #[cfg(not(windows))]
    std::fs::write(&temp, bytes)?;
    #[cfg(not(windows))]
    std::fs::rename(temp, path)
}

#[cfg(unix)]
fn thread_cpu_time_ns() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out pointer for `clock_gettime`.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts) } != 0 {
        return None;
    }
    let secs = u64::try_from(ts.tv_sec).ok()?;
    let nanos = u64::try_from(ts.tv_nsec).ok()?;
    secs.checked_mul(1_000_000_000)?.checked_add(nanos)
}

#[cfg(not(unix))]
fn thread_cpu_time_ns() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_stats_are_a_noop() {
        let stats = ConsumerStats::new(None, ProfilePipeline::Legacy);
        stats
            .write(Snapshot {
                process_id: "test",
                profile_bytes: 0,
                ring_live_bytes: 0,
                ring_peak_bytes: 0,
                reason: "test",
            })
            .unwrap();
    }

    #[test]
    fn snapshot_has_stable_schema_and_counters() {
        let dir = std::env::temp_dir().join(format!(
            "baml-obs-stats-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("thread")
        ));
        let path = dir.join("stats.json");
        let mut stats = ConsumerStats::new(Some(path.clone()), ProfilePipeline::Dual);
        stats.counters.events = 7;
        stats.counters.raw_bytes_drained = 99;
        stats.counters.dropped_dumps = 2;
        stats
            .write(Snapshot {
                process_id: "abc",
                profile_bytes: 123,
                ring_live_bytes: 456,
                ring_peak_bytes: 789,
                reason: "flush",
            })
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["pipeline"], "dual");
        assert_eq!(value["events"], 7);
        assert_eq!(value["raw_bytes_drained"], 99);
        assert_eq!(value["dropped_dumps"], 2);
        assert_eq!(value["ring_peak_bytes"], 789);
        let _ = std::fs::remove_dir_all(dir);
    }
}
