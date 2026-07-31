//! `obs-bench report` — the claim ledger (design §10.3/§10.4): group rows
//! by claim suite, render a table release claims can cite by bench id, and
//! flag rows that can never gate (extrapolated, inspected, debug builds).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::rows::{Basis, BenchRow};

pub fn render(rows: &[BenchRow]) -> String {
    let mut by_suite: BTreeMap<&str, Vec<&BenchRow>> = BTreeMap::new();
    for row in rows {
        by_suite.entry(row.suite.as_str()).or_default().push(row);
    }
    let mut out = String::new();
    let _ = writeln!(out, "claim ledger — {} rows", rows.len());
    for (suite, suite_rows) in by_suite {
        let _ = writeln!(out, "\n[{suite}]");
        let _ = writeln!(
            out,
            "  {:<44} {:<12} {:<9} {:>16} {:<7} {}",
            "bench_id", "workload", "pipeline", "value", "unit", "flags"
        );
        for row in suite_rows {
            let mut flags = Vec::new();
            match row.basis {
                Basis::Measured => {}
                Basis::Extrapolated => flags.push("extrapolated:never-gates"),
                Basis::Inspected => flags.push("inspected:never-gates"),
            }
            if row.machine.build_profile != "release" {
                flags.push("debug-build:never-gates");
            }
            let _ = writeln!(
                out,
                "  {:<44} {:<12} {:<9} {:>16.4} {:<7} {}",
                row.bench_id,
                row.workload,
                row.pipeline,
                row.value,
                row.unit,
                flags.join(",")
            );
        }
    }
    out
}

/// Load rows from NDJSON files, reporting unparseable lines to stderr.
pub fn load_rows(paths: &[std::path::PathBuf]) -> anyhow::Result<Vec<BenchRow>> {
    let mut rows = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        let (mut parsed, bad) = crate::rows::parse_ndjson(&text);
        for msg in bad {
            eprintln!("obs-bench: {}: skipping row: {msg}", path.display());
        }
        rows.append(&mut parsed);
    }
    Ok(rows)
}
