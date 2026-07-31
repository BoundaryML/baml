//! `obs-bench value-stats` — capture/byte summary of `.bamlvalue` files
//! (design §10.3: the value_stats investigation example as supported
//! tooling).
//!
//! Reports counts and bytes per record kind, plus a duplicate-content
//! estimate (sha256 over bodies): `distinct_body_bytes / body_bytes` is the
//! upper bound on what content addressing (design §7) can save — the number
//! C5 turns into a gate.

use std::{collections::HashMap, fmt::Write as _};

use anyhow::Context as _;
use bex_events::value::{ValueFileRecord, read::read_bamlvalue_from_bytes};
use sha2::{Digest as _, Sha256};

use crate::rows::{Basis, BenchRow};

#[derive(Debug, Default)]
pub struct ValueStats {
    pub files: u64,
    pub file_bytes: u64,
    pub captured_values: u64,
    pub log_events: u64,
    pub capture_losses: u64,
    pub capture_loss_skipped: u64,
    pub run_started: u64,
    pub run_completed: u64,
    /// Total inline body bytes across value + log records.
    pub body_bytes: u64,
    /// Bytes referenced through blob refs (already content-addressed).
    pub blob_refs: u64,
    /// Sum of each *distinct* body's length (sha256 identity).
    pub distinct_body_bytes: u64,
    pub distinct_bodies: u64,
    pub truncated_files: u64,
}

impl ValueStats {
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "files                {:>12}", self.files);
        let _ = writeln!(out, "file_bytes           {:>12}", self.file_bytes);
        let _ = writeln!(out, "captured_values      {:>12}", self.captured_values);
        let _ = writeln!(out, "log_events           {:>12}", self.log_events);
        let _ = writeln!(
            out,
            "capture_losses       {:>12}  (skipped={})",
            self.capture_losses, self.capture_loss_skipped
        );
        let _ = writeln!(out, "run_started          {:>12}", self.run_started);
        let _ = writeln!(out, "run_completed        {:>12}", self.run_completed);
        let _ = writeln!(out, "blob_refs            {:>12}", self.blob_refs);
        let _ = writeln!(out, "body_bytes           {:>12}", self.body_bytes);
        let _ = writeln!(
            out,
            "distinct_body_bytes  {:>12}  ({} bodies; dedupe potential {:.2}x)",
            self.distinct_body_bytes,
            self.distinct_bodies,
            if self.distinct_body_bytes == 0 {
                1.0
            } else {
                self.body_bytes as f64 / self.distinct_body_bytes as f64
            }
        );
        if self.truncated_files > 0 {
            let _ = writeln!(out, "TRUNCATED FILES      {:>12}", self.truncated_files);
        }
        out
    }

    pub fn to_rows(&self, workload: &str, pipeline: &str) -> Vec<BenchRow> {
        let dedupe = if self.distinct_body_bytes == 0 {
            1.0
        } else {
            self.body_bytes as f64 / self.distinct_body_bytes as f64
        };
        vec![
            BenchRow::new(
                "smoke.valuestats.body_bytes",
                workload,
                pipeline,
                "body_bytes",
                self.body_bytes as f64,
                "bytes",
                Basis::Measured,
            ),
            BenchRow::new(
                "smoke.valuestats.distinct_body_bytes",
                workload,
                pipeline,
                "distinct_body_bytes",
                self.distinct_body_bytes as f64,
                "bytes",
                Basis::Measured,
            ),
            BenchRow::new(
                "smoke.valuestats.dedupe_potential",
                workload,
                pipeline,
                "dedupe_potential",
                dedupe,
                "ratio",
                Basis::Measured,
            ),
        ]
    }
}

/// Aggregate the given `.bamlvalue` paths (files or directories).
pub fn run(paths: &[std::path::PathBuf]) -> anyhow::Result<ValueStats> {
    let files = crate::prof_stats::collect_files(paths, "bamlvalue")?;
    anyhow::ensure!(!files.is_empty(), "no .bamlvalue files under {paths:?}");
    let mut stats = ValueStats::default();
    let mut seen: HashMap<[u8; 32], ()> = HashMap::new();
    for file in files {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let contents = read_bamlvalue_from_bytes(&bytes)
            .with_context(|| format!("parsing {}", file.display()))?;
        stats.files += 1;
        stats.file_bytes += bytes.len() as u64;
        if contents.truncated {
            stats.truncated_files += 1;
        }
        for record in &contents.records {
            match record {
                ValueFileRecord::CapturedValue(v) => {
                    stats.captured_values += 1;
                    tally_body(&mut stats, &mut seen, &v.body);
                    if v.blob_ref.is_some() {
                        stats.blob_refs += 1;
                    }
                }
                ValueFileRecord::LogEvent(l) => {
                    stats.log_events += 1;
                    tally_body(&mut stats, &mut seen, &l.body);
                    if l.blob_ref.is_some() {
                        stats.blob_refs += 1;
                    }
                }
                ValueFileRecord::CaptureLoss(loss) => {
                    stats.capture_losses += 1;
                    stats.capture_loss_skipped += loss.skipped_count;
                }
                ValueFileRecord::RunStarted(_) => stats.run_started += 1,
                ValueFileRecord::RunCompleted(_) => stats.run_completed += 1,
            }
        }
    }
    Ok(stats)
}

fn tally_body(stats: &mut ValueStats, seen: &mut HashMap<[u8; 32], ()>, body: &[u8]) {
    if body.is_empty() {
        return;
    }
    stats.body_bytes += body.len() as u64;
    let digest: [u8; 32] = Sha256::digest(body).into();
    if seen.insert(digest, ()).is_none() {
        stats.distinct_bodies += 1;
        stats.distinct_body_bytes += body.len() as u64;
    }
}
