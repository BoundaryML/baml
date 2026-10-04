//! CLI presentation for the cloud query transport.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "query results are command output"
)]
use anyhow::{Context, Result};
use bcs_api::{
    Client, Endpoint, Secret,
    credentials::{Authentication, RequestAuthorization},
    query::{Budgets, Record, Target},
};
use serde_json::Value;

use crate::{
    ExitCode,
    query_command::{QueryArgs, QueryFormat},
};

pub(crate) fn run(
    args: &QueryArgs,
    endpoint: Endpoint,
    credential: Secret,
    project: Option<String>,
    sql: String,
) -> Result<ExitCode> {
    anyhow::ensure!(
        !args.explain && !args.internal && !args.verify && !args.no_refresh,
        "--explain, --internal, --verify and --no-refresh require --local"
    );
    let authorization = RequestAuthorization {
        authentication: Authentication::shared(endpoint.as_str(), credential),
        target: Some(Target {
            project,
            environment: args.environment.clone(),
            ..Target::default()
        }),
    };
    let client = Client::new(endpoint)?;
    let query_id = uuid::Uuid::new_v4().simple().to_string();
    let budgets = Budgets {
        max_rows: args.max_rows,
        max_wall_ms: args
            .max_wall
            .map(|duration| u64::try_from(duration.as_millis()))
            .transpose()
            .context("query wall-clock budget is too large")?,
    };
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut outcome = Value::Null;
    client.query(&authorization, &query_id, &sql, budgets, |record| {
        if args.format == QueryFormat::Jsonl {
            println!("{}", serde_json::to_string(&record)?);
        }
        match record {
            Record::Columns(value) => columns = value,
            Record::Row(value) => {
                if args.format != QueryFormat::Jsonl {
                    rows.push(value);
                }
            }
            Record::QueryOutcome(value) => outcome = value,
        }
        Ok(())
    })?;

    match args.format {
        QueryFormat::Jsonl => {}
        QueryFormat::Json => println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({"columns":columns,"rows":rows,"outcome":outcome})
            )?
        ),
        QueryFormat::Table => print_table(&columns, &rows),
    }
    match outcome["status"].as_str() {
        Some("complete") => Ok(ExitCode::Success),
        Some("truncated") => Ok(ExitCode::QueryBudgetExhausted),
        Some("failed") => {
            eprintln!("cloud query failed: {}", outcome["error"]);
            Ok(ExitCode::QueryFailed)
        }
        Some("cancelled") => Ok(ExitCode::QueryFailed),
        _ => anyhow::bail!("Boundary returned an unknown query outcome"),
    }
}

fn print_table(columns: &[Value], rows: &[Vec<Value>]) {
    let names: Vec<String> = columns
        .iter()
        .map(|column| column["name"].as_str().unwrap_or("?").to_owned())
        .collect();
    let strings: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.iter().map(|value| value.to_string()).collect())
        .collect();
    let widths: Vec<usize> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            strings
                .iter()
                .map(|row| row[i].chars().count())
                .chain([name.chars().count()])
                .max()
                .unwrap_or(0)
                .min(80)
        })
        .collect();
    for row in std::iter::once(&names).chain(strings.iter()) {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| {
                let short: String = cell.chars().take(*width).collect();
                format!(
                    "{short}{}",
                    " ".repeat(width.saturating_sub(short.chars().count()))
                )
            })
            .collect();
        println!("{}", cells.join(" | "));
    }
    eprintln!("{} row(s)", rows.len());
}
