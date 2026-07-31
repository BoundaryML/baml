use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow};
use bex_query::bql::{
    BqlCursor, ExecuteOptions, NativeBqlEngine, QueryEnvelope, ScriptResult, SnapshotToken,
    schema_json,
};
use clap::{Args, ValueEnum};
use serde_json::{Value as JsonValue, json};

#[derive(Args, Clone, Debug)]
pub(crate) struct QueryArgs {
    /// BQL source, for example: runs(latest) | calls() | top(10, by=errors)
    #[arg(value_name = "BQL")]
    query: Option<String>,

    /// Read BQL source from a file.
    #[arg(short = 'f', long = "query-file", value_name = "PATH")]
    query_file: Option<PathBuf>,

    /// Print the typed stage/field catalog as JSON.
    #[arg(long)]
    schema: bool,

    /// Parse, type-check, and print the query plan without reading run data.
    #[arg(long)]
    explain: bool,

    /// Output encoding.
    #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
    format: QueryFormat,

    /// Continue a bounded run listing from this opaque cursor.
    #[arg(long, value_name = "CURSOR")]
    cursor: Option<String>,

    /// Pin all consulted sources to a prior completeness snapshot.
    #[arg(long, value_name = "SNAPSHOT")]
    snapshot: Option<String>,

    /// Bind a BQL parameter (`$name`). Repeat for multiple parameters.
    #[arg(long = "param", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
    params: Vec<String>,

    /// Maximum rows retained in each result.
    #[arg(long, default_value_t = bex_query::bql::DEFAULT_LIMIT)]
    max_rows: usize,

    /// Maximum serialized row bytes retained in each result.
    #[arg(long, default_value_t = bex_query::DEFAULT_MAX_BYTES)]
    max_bytes: usize,

    #[arg(skip)]
    pub project: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum QueryFormat {
    #[default]
    Table,
    Json,
    Ndjson,
}

impl QueryArgs {
    pub(crate) fn run(&self) -> Result<crate::ExitCode> {
        if self.schema {
            if self.query.is_some() || self.query_file.is_some() {
                return Err(anyhow!("--schema cannot be combined with BQL input"));
            }
            println!("{}", schema_json());
            return Ok(crate::ExitCode::Success);
        }
        let source = self.query_source()?;
        let params = parse_params(&self.params)?;
        let root = self
            .project
            .clone()
            .unwrap_or(std::env::current_dir().context("failed to resolve the current directory")?);
        let engine = NativeBqlEngine::new(vec![root]);
        if self.explain {
            let plan = engine
                .explain(&source, &params)
                .map_err(|error| anyhow!("{error}"))?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
            return Ok(crate::ExitCode::Success);
        }
        let options = ExecuteOptions {
            max_rows: self.max_rows,
            max_bytes: self.max_bytes,
            cursor: self
                .cursor
                .as_deref()
                .map(BqlCursor::parse)
                .transpose()
                .map_err(|error| anyhow!("{error}"))?,
            snapshot: self
                .snapshot
                .as_deref()
                .map(SnapshotToken::parse)
                .transpose()
                .map_err(|error| anyhow!("{error}"))?,
            params,
        };
        let result = engine
            .query(&source, options)
            .map_err(|error| anyhow!("{error}"))?;
        match self.format {
            QueryFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
            QueryFormat::Ndjson => print_ndjson(&result)?,
            QueryFormat::Table => print_tables(&result),
        }
        Ok(crate::ExitCode::Success)
    }

    fn query_source(&self) -> Result<String> {
        match (&self.query, &self.query_file) {
            (Some(_), Some(_)) => Err(anyhow!(
                "provide either inline BQL or --query-file, not both"
            )),
            (Some(source), None) => Ok(source.clone()),
            (None, Some(path)) => fs::read_to_string(path)
                .with_context(|| format!("failed to read BQL file {}", path.display())),
            (None, None) => Err(anyhow!(
                "missing BQL input; pass a query, --query-file, or --schema"
            )),
        }
    }
}

fn parse_params(values: &[String]) -> Result<BTreeMap<String, String>> {
    values
        .iter()
        .map(|value| {
            let (name, value) = value
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid --param `{value}`; expected NAME=VALUE"))?;
            if name.is_empty() {
                return Err(anyhow!("parameter name cannot be empty"));
            }
            Ok((name.to_owned(), value.to_owned()))
        })
        .collect()
}

fn print_ndjson(result: &ScriptResult) -> Result<()> {
    for (index, named) in result.results.iter().enumerate() {
        let result_name = named
            .name
            .clone()
            .unwrap_or_else(|| format!("result_{index}"));
        for row in &named.result.rows {
            let mut output = row.clone();
            output.insert("_result".to_owned(), json!(&result_name));
            println!("{}", serde_json::to_string(&output)?);
        }
        println!(
            "{}",
            serde_json::to_string(&json!({
                "_result": result_name,
                "_meta": &named.result.meta,
                "_columns": &named.result.columns,
            }))?
        );
    }
    Ok(())
}

fn print_tables(result: &ScriptResult) {
    for (index, named) in result.results.iter().enumerate() {
        if result.results.len() > 1 || named.name.is_some() {
            println!(
                "{}",
                named
                    .name
                    .as_deref()
                    .map_or_else(|| format!("result_{index}"), str::to_owned)
            );
        }
        print_table(&named.result);
        println!(
            "complete={} truncated={} snapshot={}{}",
            named.result.meta.complete,
            named.result.meta.truncated,
            named.result.meta.snapshot,
            named
                .result
                .meta
                .next_cursor
                .as_deref()
                .map_or_else(String::new, |cursor| format!(" next_cursor={cursor}"))
        );
        for warning in &named.result.meta.warnings {
            println!("warning: {warning}");
        }
    }
}

fn print_table(result: &QueryEnvelope) {
    let columns = if result.columns.is_empty() {
        result
            .rows
            .iter()
            .flat_map(|row| row.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        result.columns.clone()
    };
    if columns.is_empty() {
        println!("(no rows)");
        return;
    }
    let rendered = result
        .rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|column| render_cell(row.get(column)))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let widths = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            rendered
                .iter()
                .map(|row| row[index].len())
                .max()
                .unwrap_or(0)
                .max(column.len())
                .min(40)
        })
        .collect::<Vec<_>>();
    println!("{}", render_line(&columns, &widths));
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in rendered {
        println!("{}", render_line(&row, &widths));
    }
}

fn render_cell(value: Option<&JsonValue>) -> String {
    match value {
        None | Some(JsonValue::Null) => String::new(),
        Some(JsonValue::String(value)) => value.clone(),
        Some(value) => value.to_string(),
    }
}

fn render_line(values: &[String], widths: &[usize]) -> String {
    values
        .iter()
        .zip(widths)
        .map(|(value, width)| {
            let value = truncate(value, *width);
            format!("{value:<width$}", width = *width)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    if width <= 1 {
        return "…".to_owned();
    }
    value.chars().take(width - 1).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_split_only_the_first_equals() {
        let params = parse_params(&["needle=a=b".to_owned()]).unwrap();
        assert_eq!(params["needle"], "a=b");
    }
}
