//! `baml query`: SQL over the project's local Btel recordings.
//!
//! The first query creates `<project>/.baml/btel/query.sqlite`; each query
//! then applies only newly completed recording files before running. Rows
//! and a terminal outcome (indexed extents, evidence diagnostics) are
//! printed; JSON keeps diagnostics out of row data.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "rows go to stdout and errors to stderr: that output is the command's result"
)]

use std::{io::Read as _, path::PathBuf, time::Duration};

use baml_query_btel::{Budgets, Error, Index, IndexOptions, QueryRequest, Status, catalog, format};
use clap::{Parser, ValueEnum};

use crate::project_load::find_project_root_from;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryFormat {
    Table,
    Json,
    Jsonl,
}

/// Query local recordings with SQL. BAML values navigate with brackets:
/// `SELECT output['items'][0]['name'] FROM calls WHERE args['customer']['age'] >= 30`.
#[derive(Debug, Parser)]
pub struct QueryArgs {
    /// One read-only SELECT (`-` reads stdin). See `--schema` for relations.
    pub sql: Option<String>,

    /// Print the relations, their columns and what they mean.
    #[arg(long)]
    pub schema: bool,

    /// Restrict `--schema` output to one relation.
    #[arg(long, value_name = "NAME", requires = "schema")]
    pub table: Option<String>,

    /// Output format: fixed-width table, one JSON document, or JSON lines
    /// with a terminal outcome frame.
    #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
    pub format: QueryFormat,

    /// Project directory (defaults to the current directory's project).
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// Show SQLite's plan for the translated statement instead of rows.
    #[arg(long)]
    pub explain: bool,

    /// Stop after N rows (the outcome reports the truncation).
    #[arg(long, value_name = "N")]
    pub max_rows: Option<u64>,

    /// Wall-clock budget for the SQL, e.g. `30s`, `1500ms`, or seconds.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub max_wall: Option<Duration>,

    /// Query the existing index without applying new recording files.
    #[arg(long)]
    pub no_refresh: bool,

    /// Hash every indexed file instead of trusting unchanged file metadata.
    #[arg(long)]
    pub verify: bool,

    /// Print the SQL handed to SQLite on stderr.
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

/// Value functions, capabilities and retired names, after the relations.
fn print_guide() {
    println!("BAML value functions:");
    println!(
        "  baml_value_state(v)  present, null, missing, omitted, no_value, or why the value is unavailable (cas_missing, value_truncated, capture_pending, ...)"
    );
    println!(
        "  baml_kind(v)         null, bool, int, float, string, bigint, enum, json (structured), missing, unavailable"
    );
    println!();
    println!("What recordings can answer (SELECT * FROM capabilities):");
    for (capability, support, relation, _) in catalog::CAPABILITY_ROWS {
        let relation = if relation.is_empty() {
            String::new()
        } else {
            format!("  [{relation}]")
        };
        println!("  {support:<12} {capability}{relation}");
    }
    println!();
    println!("Old tracer relations that are not available:");
    for (name, advice) in catalog::RETIRED {
        println!("  {name:<15} {advice}");
    }
}

impl QueryArgs {
    fn print_schema(&self) -> anyhow::Result<crate::ExitCode> {
        let relations: Vec<&catalog::Relation> = catalog::RELATIONS
            .iter()
            .filter(|r| {
                self.table
                    .as_deref()
                    .is_none_or(|t| r.name.eq_ignore_ascii_case(t))
            })
            .collect();
        if relations.is_empty() {
            let table = self.table.as_deref().unwrap_or_default();
            if let Some(advice) = catalog::retired(table) {
                anyhow::bail!("`{table}` from the old tracer is not available: {advice}");
            }
            anyhow::bail!("unknown relation `{table}`");
        }
        match self.format {
            QueryFormat::Json | QueryFormat::Jsonl => {
                println!("{}", serde_json::to_string_pretty(&relations)?);
            }
            QueryFormat::Table => {
                for relation in relations {
                    println!("{}: {}", relation.name, relation.doc);
                    for column in relation.columns {
                        let doc = if column.doc.is_empty() {
                            String::new()
                        } else {
                            format!("  {}", column.doc)
                        };
                        println!("  {:<26} {:<11}{doc}", column.name, column.sql_type);
                    }
                    println!();
                }
                if self.table.is_none() {
                    print_guide();
                }
            }
        }
        Ok(crate::ExitCode::Success)
    }

    pub fn run(&self) -> anyhow::Result<crate::ExitCode> {
        if self.schema {
            return self.print_schema();
        }
        let sql = match self.sql.as_deref() {
            Some("-") => {
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                text
            }
            Some(sql) => sql.to_owned(),
            None => anyhow::bail!("provide a SELECT statement, or --schema to list relations"),
        };
        let root = match find_project_root_from(self.from.as_deref())? {
            Some(root) => root,
            None => match &self.from {
                Some(from) => from.clone(),
                None => std::env::current_dir()?,
            },
        };
        let mut options = IndexOptions::default();
        options.refresh.verify_contents = self.verify;
        let request = QueryRequest {
            sql,
            params: Vec::new(),
            budgets: Budgets {
                max_rows: self.max_rows,
                max_duration: self.max_wall.or(Budgets::default().max_duration),
                ..Budgets::default()
            },
            explain: self.explain,
        };
        let outcome = Index::for_project(&root, options).and_then(|mut index| {
            if self.no_refresh {
                index.query(&request)
            } else {
                index.refresh_and_query(&request)
            }
        });
        let result = match outcome {
            Ok(result) => result,
            Err(error) => {
                eprintln!("error: {error}");
                return Ok(match error {
                    Error::Sql(_) => crate::ExitCode::QueryInvalid,
                    Error::Budget(_) => crate::ExitCode::QueryBudgetExhausted,
                    _ => crate::ExitCode::QueryFailed,
                });
            }
        };
        if self.internal {
            eprintln!("-- translated SQL\n{}", result.translated_sql);
        }
        match self.format {
            QueryFormat::Table => print!("{}", format::table(&result)),
            QueryFormat::Json => println!("{}", format::json(&result)),
            QueryFormat::Jsonl => print!("{}", format::jsonl(&result)),
        }
        if result.outcome.source_missing && self.format == QueryFormat::Table {
            eprintln!(
                "note: no recordings at {}; run BAML code in this project to record some",
                root.join(".baml/btel/recordings").display()
            );
        }
        Ok(match result.outcome.status {
            Status::Complete => crate::ExitCode::Success,
            Status::Incomplete => crate::ExitCode::QueryIncomplete,
            Status::Truncated => crate::ExitCode::QueryBudgetExhausted,
        })
    }
}
