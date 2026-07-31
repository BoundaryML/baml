//! `baml q '<query>'` — the §8 BQL CLI entry (P7).
//!
//! The same engine the web app and `/api/obs` use, over the project's
//! `.baml` artifacts. Every result carries the §8.4 completeness footer;
//! it renders as a trailing note (table) or a `footer` object (json).

// Query results are user-facing terminal output, like the other
// human-surface commands (see clean_command, describe_command).
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;
use bex_query::{BqlError, BqlTable, ColData, ObserveEngine};

use crate::project_load::find_project_root_from;

#[derive(Debug, clap::Args)]
#[command(after_help = "\
Examples:
  Top functions by total time in the latest run:
    baml q 'ctx() | top(10, by=total_ns)'

  Error hotspots:
    baml q 'ctx() | errors() | top(5, by=errors)'

  Slow contexts (p95 over 250ms):
    baml q 'ctx() | where(p95 > 250ms) | top(20, by=p95)'

  Recent runs:
    baml q 'runs(last=24h)'")]
pub struct QArgs {
    /// The BQL pipeline, e.g. 'ctx() | top(10, by=total_ns)'.
    pub query: String,

    /// Run key (a dir name under `.baml/history/` or `.baml/sessions/`).
    /// Defaults to the newest run when the query needs one.
    #[arg(long, value_name = "RUN")]
    pub run: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QFormat::Table)]
    pub format: QFormat,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum QFormat {
    Table,
    Json,
}

impl QArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let Some(project_root) = find_project_root_from(self.from.as_deref())? else {
            crate::reporter::print_error(format_args!(
                "no BAML project found (looked for `baml.toml` or `baml_src/` \
                 from the current directory upward)"
            ));
            return Ok(crate::ExitCode::Other);
        };
        let baml_dir = project_root.join(".baml");
        let mut engine = ObserveEngine::new(baml_dir.clone());

        // Default run: newest boundary, else newest session — so bare
        // `ctx()` queries work out of the box.
        let default_run = self.run.clone().or_else(|| newest_run_key(&baml_dir));
        match bex_query::bql::run(&mut engine, default_run.as_deref(), &self.query) {
            Ok(table) => {
                match self.format {
                    QFormat::Table => print!("{}", render_table(&table)),
                    QFormat::Json => println!("{}", render_json(&table)),
                }
                Ok(crate::ExitCode::Success)
            }
            Err(BqlError {
                code,
                message,
                remedy,
            }) => {
                crate::reporter::print_error(format_args!("{code}: {message}"));
                if !remedy.is_empty() {
                    eprintln!("  remedy: {remedy}");
                }
                Ok(crate::ExitCode::Other)
            }
        }
    }
}

/// Newest run key: boundaries by dir name (created_ms prefix sorts), then
/// sessions.
fn newest_run_key(baml_dir: &std::path::Path) -> Option<String> {
    for sub in ["history", "sessions"] {
        let mut names: Vec<String> = std::fs::read_dir(baml_dir.join(sub))
            .ok()?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "_unbound")
            .collect();
        names.sort();
        if let Some(newest) = names.pop() {
            return Some(newest);
        }
    }
    None
}

fn col_cell(col: &ColData, row: usize) -> String {
    match col {
        ColData::U32(v) => v.get(row).map(u32::to_string).unwrap_or_default(),
        ColData::U64(v) => v.get(row).map(u64::to_string).unwrap_or_default(),
        ColData::F64(v) => v.get(row).map(|f| format!("{f:.3}")).unwrap_or_default(),
        ColData::Str(v) => v.get(row).cloned().unwrap_or_default(),
    }
}

fn col_len(col: &ColData) -> usize {
    match col {
        ColData::U32(v) => v.len(),
        ColData::U64(v) => v.len(),
        ColData::F64(v) => v.len(),
        ColData::Str(v) => v.len(),
    }
}

pub(crate) fn render_table(table: &BqlTable) -> String {
    use std::fmt::Write as _;
    let rows = table.columns.first().map_or(0, |(_, c)| col_len(c));
    let mut widths: Vec<usize> = table.columns.iter().map(|(name, _)| name.len()).collect();
    let mut cells: Vec<Vec<String>> = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = Vec::with_capacity(table.columns.len());
        for (i, (_, col)) in table.columns.iter().enumerate() {
            let cell = col_cell(col, row);
            widths[i] = widths[i].max(cell.len());
            line.push(cell);
        }
        cells.push(line);
    }
    let mut out = String::new();
    for (i, (name, _)) in table.columns.iter().enumerate() {
        let _ = write!(out, "{:<width$}  ", name, width = widths[i]);
    }
    out.push('\n');
    for line in cells {
        for (i, cell) in line.iter().enumerate() {
            let _ = write!(out, "{:<width$}  ", cell, width = widths[i]);
        }
        out.push('\n');
    }
    let f = &table.footer;
    let _ = write!(
        out,
        "-- {} row(s); sealed={} torn={}",
        rows, f.sealed, f.torn
    );
    if !f.degraded.is_empty() {
        let _ = write!(out, "; notes: {}", f.degraded.join("; "));
    }
    out.push('\n');
    out
}

pub(crate) fn render_json(table: &BqlTable) -> String {
    let rows = table.columns.first().map_or(0, |(_, c)| col_len(c));
    let mut out_rows = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut obj = serde_json::Map::new();
        for (name, col) in &table.columns {
            let value = match col {
                ColData::U32(v) => serde_json::json!(v.get(row)),
                ColData::U64(v) => serde_json::json!(v.get(row)),
                ColData::F64(v) => serde_json::json!(v.get(row)),
                ColData::Str(v) => serde_json::json!(v.get(row)),
            };
            obj.insert(name.clone(), value);
        }
        out_rows.push(serde_json::Value::Object(obj));
    }
    let f = &table.footer;
    serde_json::json!({
        "rows": out_rows,
        "footer": {
            "sealed": f.sealed,
            "torn": f.torn,
            "first_ts_ns": f.first_ts_ns,
            "last_ts_ns": f.last_ts_ns,
            "degraded": f.degraded,
        },
    })
    .to_string()
}
