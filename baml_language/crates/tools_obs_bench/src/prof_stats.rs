//! `obs-bench prof-stats` — per-function aggregate summary of one or more
//! `.bamlprof` files (design §10.3: the uncommitted investigation example,
//! recreated as supported tooling).
//!
//! One streaming pass, O(open calls) memory: `CallFunction`/`EndFunction`
//! are matched by `(thread_id, call_id)`; durations accumulate per
//! `function_id`. Names resolve through the header's function table.

use std::{collections::HashMap, fmt::Write as _, path::Path};

use anyhow::Context as _;
use bex_events::prof::{
    pb,
    read::{BamlprofContents, read_bamlprof_from_bytes},
};

use crate::rows::{Basis, BenchRow};

/// Per-function aggregates from one pass.
#[derive(Debug, Default, Clone)]
pub struct FnAgg {
    pub calls: u64,
    pub ends_ok: u64,
    pub ends_err: u64,
    pub ends_cancel: u64,
    pub ends_exit: u64,
    pub total_ns: u64,
}

/// Whole-file aggregates.
#[derive(Debug, Default)]
pub struct ProfStats {
    pub events: u64,
    pub threads_started: u64,
    pub threads_ended: u64,
    pub heartbeats: u64,
    pub set_function_ids: u64,
    /// Calls that never saw an `EndFunction` (open at capture end).
    pub open_calls: u64,
    /// `EndFunction` without a matching open call (cross-file drains, bugs).
    pub orphan_ends: u64,
    pub by_function: HashMap<u32, FnAgg>,
    /// function_id -> fqn from the header table.
    pub names: HashMap<u32, String>,
    /// Raw file size, for bytes/event context.
    pub file_bytes: u64,
}

impl ProfStats {
    /// Aggregate one parsed `.bamlprof`.
    pub fn accumulate(&mut self, contents: &BamlprofContents, file_bytes: u64) {
        self.file_bytes += file_bytes;
        if let Some(table) = &contents.header.function_table {
            for f in &table.functions {
                self.names
                    .entry(f.function_id)
                    .or_insert_with(|| f.fqn.clone());
            }
        }
        // (thread_id, call_id) -> (function_id, start_ns)
        let mut open: HashMap<(u64, u64), (u32, u64)> = HashMap::new();
        for event in &contents.events {
            self.events += 1;
            match &event.event {
                Some(pb::disk_event_v1::Event::CallFunction(c)) => {
                    let agg = self.by_function.entry(c.function_id).or_default();
                    agg.calls += 1;
                    open.insert((c.thread_id, c.call_id), (c.function_id, c.timestamp_ns));
                }
                Some(pb::disk_event_v1::Event::EndFunction(e)) => {
                    if let Some((function_id, start_ns)) = open.remove(&(e.thread_id, e.call_id)) {
                        let agg = self.by_function.entry(function_id).or_default();
                        match e.status() {
                            pb::FunctionEndStatus::Ok => agg.ends_ok += 1,
                            pb::FunctionEndStatus::Errored => agg.ends_err += 1,
                            pb::FunctionEndStatus::Cancelled => agg.ends_cancel += 1,
                            pb::FunctionEndStatus::Exited => agg.ends_exit += 1,
                        }
                        agg.total_ns += e.timestamp_ns.saturating_sub(start_ns);
                    } else {
                        self.orphan_ends += 1;
                    }
                }
                Some(pb::disk_event_v1::Event::StartThread(_)) => self.threads_started += 1,
                Some(pb::disk_event_v1::Event::EndThread(_)) => self.threads_ended += 1,
                Some(pb::disk_event_v1::Event::Heartbeat(_)) => self.heartbeats += 1,
                Some(pb::disk_event_v1::Event::SetFunctionId(_)) => self.set_function_ids += 1,
                // §5.3/§5.4 enrichment events: irrelevant to per-function
                // aggregates here (the CCT engine consumes them); counted
                // via self.events only.
                Some(
                    pb::disk_event_v1::Event::SuspendThread(_)
                    | pb::disk_event_v1::Event::ResumeThread(_)
                    | pb::disk_event_v1::Event::LlmCallMeta(_)
                    | pb::disk_event_v1::Event::ModelBirth(_),
                ) => {}
                // Unknown/None: forward-compat — count the event, skip the body.
                None => {}
            }
        }
        self.open_calls += open.len() as u64;
    }

    fn name(&self, function_id: u32) -> String {
        self.names
            .get(&function_id)
            .cloned()
            .unwrap_or_else(|| format!("fn#{function_id}"))
    }

    /// Human table, sorted by total time descending.
    pub fn render_table(&self) -> String {
        let mut rows: Vec<(&u32, &FnAgg)> = self.by_function.iter().collect();
        rows.sort_by(|a, b| b.1.total_ns.cmp(&a.1.total_ns).then(a.0.cmp(b.0)));
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<52} {:>12} {:>10} {:>8} {:>8} {:>14}",
            "function", "calls", "ok", "err", "cancel", "total_ms"
        );
        for (id, agg) in rows {
            let _ = writeln!(
                out,
                "{:<52} {:>12} {:>10} {:>8} {:>8} {:>14.3}",
                truncate(&self.name(*id), 52),
                agg.calls,
                agg.ends_ok,
                agg.ends_err,
                agg.ends_cancel,
                agg.total_ns as f64 / 1e6,
            );
        }
        let _ = writeln!(
            out,
            "\nevents={} threads={}/{} heartbeats={} set_ids={} open_calls={} orphan_ends={} file_bytes={} bytes/event={:.1}",
            self.events,
            self.threads_started,
            self.threads_ended,
            self.heartbeats,
            self.set_function_ids,
            self.open_calls,
            self.orphan_ends,
            self.file_bytes,
            if self.events == 0 {
                0.0
            } else {
                self.file_bytes as f64 / self.events as f64
            },
        );
        out
    }

    /// Context rows for `report` (ungated; basis=measured, suite=smoke).
    pub fn to_rows(&self, workload: &str, pipeline: &str) -> Vec<BenchRow> {
        let total_calls: u64 = self.by_function.values().map(|a| a.calls).sum();
        vec![
            BenchRow::new(
                "smoke.profstats.events",
                workload,
                pipeline,
                "events",
                self.events as f64,
                "count",
                Basis::Measured,
            ),
            BenchRow::new(
                "smoke.profstats.calls",
                workload,
                pipeline,
                "calls",
                total_calls as f64,
                "count",
                Basis::Measured,
            ),
            BenchRow::new(
                "smoke.profstats.file_bytes",
                workload,
                pipeline,
                "file_bytes",
                self.file_bytes as f64,
                "bytes",
                Basis::Measured,
            ),
            BenchRow::new(
                "smoke.profstats.bytes_per_event",
                workload,
                pipeline,
                "bytes_per_event",
                if self.events == 0 {
                    0.0
                } else {
                    self.file_bytes as f64 / self.events as f64
                },
                "bytes",
                Basis::Measured,
            ),
        ]
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("…{}", &s[s.len() - (max - 1)..])
    }
}

/// Load and aggregate the given `.bamlprof` paths (files or directories).
pub fn run(paths: &[std::path::PathBuf]) -> anyhow::Result<ProfStats> {
    let mut stats = ProfStats::default();
    let files = collect_files(paths, "bamlprof")?;
    anyhow::ensure!(!files.is_empty(), "no .bamlprof files under {paths:?}");
    for file in files {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let contents = read_bamlprof_from_bytes(&bytes)
            .with_context(|| format!("parsing {}", file.display()))?;
        stats.accumulate(&contents, bytes.len() as u64);
    }
    Ok(stats)
}

/// Expand files/dirs into files with the given extension, sorted.
pub fn collect_files(
    paths: &[std::path::PathBuf],
    ext: &str,
) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            collect_dir(path, ext, &mut files)?;
        } else if path.is_file() {
            files.push(path.clone());
        } else {
            anyhow::bail!("no such file or directory: {}", path.display());
        }
    }
    files.sort();
    Ok(files)
}

fn collect_dir(dir: &Path, ext: &str, out: &mut Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_dir(&path, ext, out)?;
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
    Ok(())
}
