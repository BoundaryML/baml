//! The one NDJSON row schema every obs bench emits and `obs-bench report`
//! consumes (design §10.3). A row is a single named measurement with enough
//! context to be compared across runs, machines, and pipelines.
//!
//! Honest-measurement rules (§10.4) are encoded in the schema:
//! - `basis` tags every row `measured | extrapolated | inspected`; **only
//!   `measured` rows may gate** (`check` refuses to gate anything else).
//! - `machine` embeds the manifest so no cross-machine absolute comparison
//!   can happen by accident (check compares same-platform baselines only).
//! - `pipeline` names the `BAML_PROFILE_PIPELINE` mode the row measured
//!   (post-P9 always `cct` for new rows; historical rows keep their
//!   `legacy`/`dual` labels).

use serde::{Deserialize, Serialize};

use crate::machine::MachineManifest;

/// Schema tag for [`BenchRow`] lines.
pub const BENCH_ROW_SCHEMA: &str = "baml.obs.bench_row.v1";

/// How a number was obtained. Only `Measured` rows can gate (§10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Directly measured in this process/run.
    Measured,
    /// Computed from measured inputs (e.g. extrapolated daily volume).
    Extrapolated,
    /// Asserted by inspection (named test / code audit), not a number.
    Inspected,
}

/// One measurement row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRow {
    /// Always [`BENCH_ROW_SCHEMA`].
    pub schema: String,
    /// Stable id, `<claim>.<workload>.<metric>` (e.g. `c3.hotloop.bytes_per_s`).
    /// Release claims cite these ids (§10.4).
    pub bench_id: String,
    /// Claim family: `c1`..`c13`, or `smoke` for ungated context rows.
    pub suite: String,
    /// Workload name (`hotloop`, `agent_like`, ...).
    pub workload: String,
    /// `BAML_PROFILE_PIPELINE` mode measured (`cct` post-P9; historical
    /// rows may carry `legacy` | `dual`).
    pub pipeline: String,
    /// Metric name (`bytes_per_s`, `ns_per_call`, `consumer_cpu_ms_per_mcall`, ...).
    pub metric: String,
    /// The value, in `unit`s.
    pub value: f64,
    /// Unit string (`B/s`, `ns`, `ms`, `bytes`, `ratio`).
    pub unit: String,
    /// How the number was obtained; only `measured` rows gate.
    pub basis: Basis,
    /// Machine context (embedded, not referenced — rows are self-contained).
    pub machine: MachineManifest,
    /// Git commit of the tree that produced the row, when resolvable.
    pub git_sha: Option<String>,
    /// Wall-clock ms since the Unix epoch at row creation.
    pub created_ms: u64,
    /// Free-form context (workload knobs, caveats). Never parsed by check.
    pub notes: Option<String>,
}

impl BenchRow {
    /// A builder with the environment-derived fields filled in.
    pub fn new(
        bench_id: impl Into<String>,
        workload: impl Into<String>,
        pipeline: impl Into<String>,
        metric: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        basis: Basis,
    ) -> BenchRow {
        let bench_id = bench_id.into();
        let suite = bench_id.split('.').next().unwrap_or("smoke").to_string();
        BenchRow {
            schema: BENCH_ROW_SCHEMA.to_string(),
            bench_id,
            suite,
            workload: workload.into(),
            pipeline: pipeline.into(),
            metric: metric.into(),
            value,
            unit: unit.into(),
            basis,
            machine: MachineManifest::collect(),
            git_sha: crate::machine::git_sha(),
            created_ms: now_epoch_ms(),
            notes: None,
        }
    }

    /// Attach free-form notes.
    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> BenchRow {
        self.notes = Some(notes.into());
        self
    }

    /// Serialize as one NDJSON line (no trailing newline).
    pub fn to_ndjson(&self) -> String {
        serde_json::to_string(self).expect("BenchRow is always serializable")
    }
}

/// Parse NDJSON lines into rows, skipping (and returning) unparseable lines.
pub fn parse_ndjson(input: &str) -> (Vec<BenchRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut bad = Vec::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<BenchRow>(line) {
            Ok(row) if row.schema == BENCH_ROW_SCHEMA => rows.push(row),
            Ok(row) => bad.push(format!("unknown schema {:?}", row.schema)),
            Err(err) => bad.push(format!("{err}: {line}")),
        }
    }
    (rows, bad)
}

pub(crate) fn now_epoch_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_roundtrips_through_ndjson() {
        let row = BenchRow::new(
            "c3.hotloop.bytes_per_s",
            "hotloop",
            "legacy",
            "bytes_per_s",
            446_000_000.0,
            "B/s",
            Basis::Measured,
        )
        .with_notes("iters=18100000");
        let line = row.to_ndjson();
        let (rows, bad) = parse_ndjson(&line);
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bench_id, "c3.hotloop.bytes_per_s");
        assert_eq!(rows[0].suite, "c3");
        assert_eq!(rows[0].basis, Basis::Measured);
    }

    #[test]
    fn parse_reports_bad_lines_without_dropping_good_ones() {
        let row = BenchRow::new("c1.x.y", "x", "legacy", "y", 1.0, "ns", Basis::Measured);
        let input = format!("not json\n{}\n\n", row.to_ndjson());
        let (rows, bad) = parse_ndjson(&input);
        assert_eq!(rows.len(), 1);
        assert_eq!(bad.len(), 1);
    }
}
