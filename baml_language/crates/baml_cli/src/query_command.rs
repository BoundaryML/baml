//! Profile queries are unavailable until a supported profiling backend exists.

use std::{path::PathBuf, time::Duration};

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryFormat {
    Table,
    Json,
    Jsonl,
}

/// Retired profile query command; reports that profiling is unavailable.
#[derive(Debug, Parser)]
pub struct QueryArgs {
    /// Portable SQL against the versioned catalog (`-` reads stdin; see
    /// `--schema` and `baml describe query`).
    pub sql: Option<String>,

    /// Print the catalog profile (relations, views, columns, docs).
    #[arg(long)]
    pub schema: bool,

    /// Restrict `--schema` output to one relation or view.
    #[arg(long, value_name = "NAME", requires = "schema")]
    pub table: Option<String>,

    /// Output format: fixed-width table, one JSON envelope, or JSON lines
    /// with a terminal outcome frame.
    #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
    pub format: QueryFormat,

    /// Project directory (defaults to the current directory's project).
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// Plan the statement without executing it (wraps it in EXPLAIN).
    #[arg(long)]
    pub explain: bool,

    /// Result-row budget (terminal E_QUERY_BUDGET_EXCEEDED when hit).
    #[arg(long, value_name = "N")]
    pub max_rows: Option<u64>,

    /// Wall-clock budget, e.g. `30s`, `1500ms`, or plain seconds.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub max_wall: Option<Duration>,

    /// Show internal relations too (`BAML_INTERNAL=1` does the same).
    #[arg(long)]
    pub internal: bool,
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| e.to_string());
    }
    let seconds = value.strip_suffix('s').unwrap_or(value).trim();
    seconds
        .parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|e| e.to_string())
}

impl QueryArgs {
    pub fn run(&self) -> anyhow::Result<crate::ExitCode> {
        anyhow::bail!("profiling is unavailable: the old runtime tracing pipeline has been removed")
    }
}
