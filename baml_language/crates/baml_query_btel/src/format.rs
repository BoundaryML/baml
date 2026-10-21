//! Text renderings of a query result. Diagnostics stay out of row data: the
//! table form prints them after the rows, JSON keeps them in `outcome`.
use std::fmt::Write as _;

use serde_json::{Value as Json, json};

use crate::query::{QueryResult, Status};

fn text(value: &Json) -> String {
    match value {
        Json::Null => "NULL".into(),
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// One JSON document: columns, rows, and the terminal outcome.
pub fn json(result: &QueryResult) -> String {
    serde_json::to_string_pretty(&json!({
        "columns": result.columns,
        "rows": result.rows,
        "outcome": result.outcome,
    }))
    .expect("serializable result")
}

/// One JSON object per row, then a terminal `{"outcome": ...}` frame.
pub fn jsonl(result: &QueryResult) -> String {
    let mut out = String::new();
    for row in &result.rows {
        let object: serde_json::Map<String, Json> = result
            .columns
            .iter()
            .zip(row)
            .map(|(column, value)| (column.name.clone(), value.clone()))
            .collect();
        out.push_str(&Json::Object(object).to_string());
        out.push('\n');
    }
    out.push_str(&json!({ "outcome": result.outcome }).to_string());
    out.push('\n');
    out
}

/// Fixed-width table followed by a short outcome summary.
pub fn table(result: &QueryResult) -> String {
    const MAX_CELL: usize = 80;
    let header: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
    let cells: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| {
                    let mut cell = text(value).replace('\n', "\\n");
                    if cell.chars().count() > MAX_CELL {
                        cell = cell.chars().take(MAX_CELL - 1).collect::<String>() + "…";
                    }
                    cell
                })
                .collect()
        })
        .collect();
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let line = |out: &mut String, values: &[String]| {
        let parts: Vec<String> = values
            .iter()
            .zip(&widths)
            .map(|(value, width)| format!("{value:<width$}"))
            .collect();
        out.push_str(parts.join(" | ").trim_end());
        out.push('\n');
    };
    line(&mut out, &header);
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    out.push_str(&rule.join("-+-"));
    out.push('\n');
    for row in &cells {
        line(&mut out, row);
    }
    let outcome = &result.outcome;
    let _ = write!(
        out,
        "({} row{}; {}",
        result.rows.len(),
        if result.rows.len() == 1 { "" } else { "s" },
        match outcome.status {
            Status::Complete => "complete",
            Status::Incomplete => "incomplete: some evidence was unavailable",
            Status::Truncated => "truncated by a budget",
        }
    );
    if outcome.source_missing {
        out.push_str("; no recordings in this project yet");
    }
    let indexed: i64 = outcome.recordings.iter().map(|r| r.indexed_sequence).sum();
    let _ = write!(
        out,
        "; {} recording{}, {indexed} file{} indexed",
        outcome.recordings.len(),
        if outcome.recordings.len() == 1 {
            ""
        } else {
            "s"
        },
        if indexed == 1 { "" } else { "s" }
    );
    if outcome.unsealed {
        out.push_str("; unsealed: the prefix may grow");
    }
    out.push_str(")\n");
    for diagnostic in &outcome.diagnostics {
        let _ = writeln!(
            out,
            "warning[{}] x{}: {}",
            diagnostic.code, diagnostic.count, diagnostic.message
        );
    }
    out
}
