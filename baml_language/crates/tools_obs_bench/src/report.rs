use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::dataset;

#[derive(Debug, Serialize)]
pub(crate) struct LedgerRow {
    claim: String,
    bench_id: String,
    evidence: String,
    metrics: BTreeMap<String, f64>,
    source: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClaimLedger {
    schema_version: u32,
    rows: Vec<LedgerRow>,
}

pub(crate) fn build(paths: &[PathBuf]) -> Result<ClaimLedger> {
    dataset::validate(paths)?;
    let rows = dataset::load(paths)?;
    let mut ledger = Vec::new();
    for row in rows {
        let Some(bench_id) = row.bench_id() else {
            continue;
        };
        let claim = claim_for(bench_id).to_owned();
        let metrics = row
            .object()?
            .iter()
            .filter_map(|(name, value)| value.as_f64().map(|value| (name.clone(), value)))
            .filter(|(name, _)| name != "schema_version")
            .collect();
        ledger.push(LedgerRow {
            claim,
            bench_id: bench_id.to_owned(),
            evidence: row.evidence().unwrap_or("unclassified").to_owned(),
            metrics,
            source: row.source.display().to_string(),
        });
    }
    ledger
        .sort_by(|left, right| (&left.claim, &left.bench_id).cmp(&(&right.claim, &right.bench_id)));
    Ok(ClaimLedger {
        schema_version: 1,
        rows: ledger,
    })
}

pub(crate) fn markdown(ledger: &ClaimLedger) -> String {
    let mut output = String::from(
        "# Observability claim ledger\n\n| Claim | Bench | Evidence | Key metrics |\n|---|---|---|---|\n",
    );
    for row in &ledger.rows {
        let metrics = row
            .metrics
            .iter()
            .take(6)
            .map(|(name, value)| format!("{name}={value:.3}"))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            row.claim, row.bench_id, row.evidence, metrics
        ));
    }
    output
}

fn claim_for(bench_id: &str) -> &'static str {
    if bench_id.starts_with("cct_engine") {
        "C1/C2"
    } else if bench_id.starts_with("bcct_encode") {
        "C3/C4"
    } else if bench_id.starts_with("bcct_recovery") {
        "C8"
    } else if bench_id.starts_with("value_cas") {
        "C5/C10"
    } else if bench_id.starts_with("c6_") {
        "C6"
    } else if bench_id.starts_with("query_") {
        "C7"
    } else if bench_id.starts_with("c11_") {
        "C11"
    } else if bench_id.starts_with("c12_") {
        "C12"
    } else if bench_id.starts_with("c13_") {
        "C13"
    } else {
        "tracked"
    }
}

#[cfg(test)]
mod tests {
    use super::claim_for;

    #[test]
    fn permanent_gate_rows_are_not_left_unclassified() {
        for (bench_id, claim) in [
            ("c11_exact_index_100k", "C11"),
            ("c11_partition_lifecycle_10k", "C11"),
            ("c12_async_fsync_stall", "C12"),
            ("c13_live_wire_hotloop", "C13"),
        ] {
            assert_eq!(claim_for(bench_id), claim);
        }
    }
}
