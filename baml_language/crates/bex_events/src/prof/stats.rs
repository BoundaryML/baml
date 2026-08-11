//! Consumer self-reporting (observability design §10.3, `BAML_OBS_STATS`).
//!
//! The single most important benchmark addition: the prof consumer (and
//! later the value drain service) reports its own thread CPU, event/byte
//! throughput, and flush/sync activity as data, so consumer-cost claims
//! (C2/C10/C12) are measurements, not inferences from wall-clock deltas.
//!
//! One NDJSON line per report, appended to the `BAML_OBS_STATS` path. Lines
//! are self-describing (`schema` field) and cumulative-since-consumer-start:
//! readers take the *last* line per `(pid, thread)` for end-of-run totals,
//! or diff successive lines for interval rates. Appending (vs overwrite)
//! keeps the file usable when several processes share one path — writes of
//! a line this small are atomic in practice on POSIX (O_APPEND).
//!
//! Never on a hot path: reports happen at flush/engine-close cadence.
//! Failures are reported once to stderr and disable further attempts —
//! self-reporting must never break the host.

use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

/// Cumulative counters for one consumer thread. Plain `u64`s — the consumer
/// is single-threaded, so no atomics.
#[derive(Debug, Default, Clone)]
pub(crate) struct ConsumerCounters {
    /// Drained ranges handed to `transcode` (one per `Registry::sweep` hit).
    pub ranges: u64,
    /// Raw ring bytes drained.
    pub bytes_drained: u64,
    /// Raw records decoded.
    pub records: u64,
    /// Ranges that hit a decode error (rest of range dropped).
    pub corrupt_ranges: u64,
    /// Protobuf events appended to `.bamlprof` writers.
    pub events_encoded: u64,
    /// Heartbeat events stamped.
    pub heartbeats: u64,
    /// Buffered-flush calls (`flush_buffered` / idle `flush_files`).
    pub flushes: u64,
    /// Durable syncs (`fsync`) issued.
    pub syncs: u64,
    /// Engines closed (files sealed).
    pub engines_closed: u64,
    /// Sweeps that made progress.
    pub sweeps_with_progress: u64,
    /// Records dropped because their engine was already closed.
    pub records_after_close_ranges: u64,
    /// CCT pipeline (design §5), when running: live nodes across engines.
    pub cct_nodes: u64,
    /// CCT deferral activity (§5.2) — cumulative.
    pub cct_deferred: u64,
    /// CCT synthesized parents (§5.2 resync) — nonzero means degraded data.
    pub cct_synthesized: u64,
    /// CCT recent-ring evictions (§5.8).
    pub cct_evicted_calls: u64,
    /// §5.9 flight-recorder dumps written.
    pub flight_dumps: u64,
    /// Records producers dropped under the structural-exhaustion policy.
    pub shed_records: u64,
}

/// Where and how to report. Owned by the consumer thread.
pub(crate) struct StatsReporter {
    path: Option<PathBuf>,
    /// Disabled after the first write failure (reported once).
    failed: bool,
    started: Instant,
    pipeline: &'static str,
    thread_name: &'static str,
}

impl StatsReporter {
    pub(crate) fn new(
        path: Option<PathBuf>,
        pipeline: &'static str,
        thread_name: &'static str,
    ) -> StatsReporter {
        StatsReporter {
            path,
            failed: false,
            started: Instant::now(),
            pipeline,
            thread_name,
        }
    }

    /// Is reporting configured (and not failed)?
    pub(crate) fn active(&self) -> bool {
        self.path.is_some() && !self.failed
    }

    /// Append one cumulative NDJSON line. Cheap no-op when unconfigured.
    pub(crate) fn report(&mut self, counters: &ConsumerCounters, live_ring_bytes: usize) {
        let Some(path) = self.path.clone() else {
            return;
        };
        if self.failed {
            return;
        }
        let line = self.render(counters, live_ring_bytes);
        if let Err(err) = append_line(&path, &line) {
            self.failed = true;
            super::consumer::report_public(format_args!(
                "cannot append consumer stats to {}; disabling self-report: {err}",
                path.display()
            ));
        }
    }

    fn render(&self, c: &ConsumerCounters, live_ring_bytes: usize) -> String {
        // serde_json for correctness of escaping; the map is tiny and cold.
        serde_json::json!({
            "schema": "baml.obs.consumer_stats.v1",
            "pid": std::process::id(),
            "thread": self.thread_name,
            "pipeline": self.pipeline,
            "wall_ns": u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            "cpu_ns": thread_cpu_ns(),
            "ranges": c.ranges,
            "bytes_drained": c.bytes_drained,
            "records": c.records,
            "corrupt_ranges": c.corrupt_ranges,
            "events_encoded": c.events_encoded,
            "heartbeats": c.heartbeats,
            "flushes": c.flushes,
            "syncs": c.syncs,
            "engines_closed": c.engines_closed,
            "sweeps_with_progress": c.sweeps_with_progress,
            "records_after_close_ranges": c.records_after_close_ranges,
            "cct_nodes": c.cct_nodes,
            "cct_deferred": c.cct_deferred,
            "cct_synthesized": c.cct_synthesized,
            "cct_evicted_calls": c.cct_evicted_calls,
            "flight_dumps": c.flight_dumps,
            "shed_records": c.shed_records,
            "live_ring_bytes": live_ring_bytes,
        })
        .to_string()
    }
}

fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    // One write call for line + newline: keeps concurrent writers' lines
    // whole (POSIX O_APPEND atomicity for small writes).
    let mut buf = String::with_capacity(line.len() + 1);
    buf.push_str(line);
    buf.push('\n');
    file.write_all(buf.as_bytes())
}

/// CPU time consumed by the *calling thread* (user+sys), in nanoseconds.
/// Returns 0 where unsupported (non-unix); readers treat 0 as "unavailable"
/// (a real consumer thread always has nonzero CPU by its first report).
#[cfg(unix)]
#[expect(
    unsafe_code,
    reason = "libc clock_gettime FFI; no aliasing, out-param only"
)]
pub(crate) fn thread_cpu_ns() -> u64 {
    // SAFETY: clock_gettime with a zeroed out-param is the documented usage;
    // CLOCK_THREAD_CPUTIME_ID exists on Linux and macOS.
    unsafe {
        let mut ts: libc::timespec = std::mem::zeroed();
        if libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts) == 0 {
            u64::try_from(ts.tv_sec)
                .unwrap_or(0)
                .saturating_mul(1_000_000_000)
                .saturating_add(u64::try_from(ts.tv_nsec).unwrap_or(0))
        } else {
            0
        }
    }
}

#[cfg(not(unix))]
pub(crate) fn thread_cpu_ns() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_cpu_advances_with_work() {
        let before = thread_cpu_ns();
        // Burn a little CPU deterministically.
        let mut acc = 0u64;
        for i in 0..2_000_000u64 {
            acc = acc.wrapping_mul(31).wrapping_add(i);
        }
        std::hint::black_box(acc);
        let after = thread_cpu_ns();
        if cfg!(unix) {
            assert!(
                after > before,
                "thread CPU must advance: {before} -> {after}"
            );
        }
    }

    #[test]
    fn report_appends_parseable_ndjson() {
        let dir = std::env::temp_dir().join(format!("baml-obs-stats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stats.ndjson");
        let _ = std::fs::remove_file(&path);

        let mut reporter = StatsReporter::new(Some(path.clone()), "legacy", "test-thread");
        let mut counters = ConsumerCounters::default();
        counters.records = 42;
        counters.bytes_drained = 1234;
        reporter.report(&counters, 65536);
        counters.records = 43;
        reporter.report(&counters, 0);

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "two reports = two lines: {contents:?}");
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["schema"], "baml.obs.consumer_stats.v1");
            assert_eq!(v["thread"], "test-thread");
            assert_eq!(v["pipeline"], "legacy");
        }
        let last: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(last["records"], 43);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unconfigured_reporter_is_inert() {
        let mut reporter = StatsReporter::new(None, "legacy", "t");
        assert!(!reporter.active());
        reporter.report(&ConsumerCounters::default(), 0);
    }
}
