//! `baml local recordings` and `baml cloud recordings`: SQL over Btel
//! recordings. The two take the same arguments and return the same report;
//! the namespace picks the backend. `baml query` is the shortcut that picks
//! for you.
//!
//! The first local query creates `<project>/.baml/btel/query.sqlite`; each
//! query then applies only newly completed recording files before running.

use std::{
    io::{self, Read as _, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context as _;
use baml_query_btel::{Budgets, Error, Index, IndexOptions, QueryRequest, Status, catalog};
use bcs_api::{
    credentials::{Authentication, RequestAuthorization},
    diagnostics::{Context as DiagnosticContext, CredentialSource, Operation, Outcome as Failure},
    query::{Record, Target},
};
use clap::{Args, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    ExitCode,
    output::{Command, CommandContext, Outcome, Report, Style, run_command},
    project_load::find_project_root_from,
};

#[derive(Debug, Subcommand)]
pub(crate) enum RecordingsCommands {
    /// Run one read-only SELECT over recordings.
    #[command(after_long_help = QUERY_HELP)]
    Query(QueryArgs),
    /// Print the relations, their columns and what they mean.
    #[command(after_long_help = "\
Examples:
  Every relation:
    baml local recordings schema

  One relation:
    baml local recordings schema spans")]
    Schema(SchemaArgs),
}

impl RecordingsCommands {
    pub(crate) fn run(
        &self,
        cx: &CommandContext,
        backend: Backend<'_>,
    ) -> anyhow::Result<ExitCode> {
        match self {
            Self::Query(args) => run_command(&Query { args, backend }, cx),
            Self::Schema(args) => run_command(args, cx),
        }
    }
}

pub(crate) const QUERY_HELP: &str = "\
BAML values navigate with brackets:
  SELECT output_value['items'][0]['name'] FROM spans WHERE input_args['customer']['age'] >= 30

Exit codes:
  0  complete
  1  incomplete: rows returned, some evidence was unavailable
  2  invalid SQL, or a relation you may not read
  3  a budget (--max-rows, --max-wall) was exhausted
  4  cancelled
  5  any other failure

Examples:
  Local recordings:
    baml local recordings query \"SELECT span_name FROM spans LIMIT 5\"

  Boundary Cloud:
    baml cloud recordings query \"SELECT span_name FROM spans LIMIT 5\"

  Whichever you are set up for (cloud when logged in, otherwise local):
    baml query \"SELECT span_name FROM spans LIMIT 5\"";

#[derive(Debug, Args)]
pub(crate) struct QueryArgs {
    /// One read-only SELECT (`-` reads stdin). See `recordings schema` for relations.
    pub sql: String,

    /// Stop after N rows (the outcome reports the truncation).
    #[arg(long, value_name = "N")]
    pub max_rows: Option<u64>,

    /// Wall-clock budget for the SQL, e.g. `30s`, `1500ms`, or seconds.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration)]
    pub max_wall: Option<Duration>,
}

#[derive(Debug, Args)]
pub(crate) struct SchemaArgs {
    /// Restrict the output to one relation.
    pub table: Option<String>,
}

/// Which Boundary Cloud project and environment a cloud command targets.
/// Defaults come from `[boundary]` in `baml.toml` and `BOUNDARY_PROJECT`.
#[derive(Debug, Args, Clone, Default)]
pub(crate) struct CloudScope {
    /// Boundary Cloud project in org_handle/project_name form.
    #[arg(long, global = true, value_name = "ORG/PROJECT")]
    pub project: Option<String>,

    /// Boundary Cloud environment; defaults to your personal environment.
    #[arg(long, global = true, value_name = "ENV")]
    pub env: Option<String>,
}

/// Where a query runs.
pub(crate) enum Backend<'a> {
    Local,
    Cloud(&'a CloudScope),
    /// `baml query`: cloud when logged in or `BOUNDARY_API_KEY` is set,
    /// otherwise local. Says which on stderr.
    Auto,
}

pub(crate) struct Query<'a> {
    pub args: &'a QueryArgs,
    pub backend: Backend<'a>,
}

/// Rows plus the terminal outcome (indexed extents, evidence diagnostics).
/// Diagnostics stay out of row data.
#[derive(Debug, Serialize)]
pub(crate) struct QueryReport {
    pub columns: Vec<Value>,
    pub rows: Vec<Vec<Value>>,
    pub outcome: Value,
}

impl Report for QueryReport {
    const JSONL: bool = true;

    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        const MAX_CELL: usize = 80;
        // A failed query has no result to show; the error is on stderr.
        if self.columns.is_empty() {
            return Ok(());
        }
        let header: Vec<String> = self.column_names().map(str::to_owned).collect();
        let cells: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| {
                        let mut cell = match value {
                            Value::Null => "NULL".to_owned(),
                            Value::String(text) => text.replace('\n', "\\n"),
                            other => other.to_string(),
                        };
                        if cell.chars().count() > MAX_CELL {
                            cell = cell.chars().take(MAX_CELL - 3).collect::<String>() + "...";
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
        let mut line = |values: &[String]| {
            let parts: Vec<String> = values
                .iter()
                .zip(&widths)
                .map(|(value, width)| format!("{value:<width$}"))
                .collect();
            writeln!(out, "{}", parts.join(" | ").trim_end())
        };
        line(&header)?;
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        line(&[rule.join("-+-")])?;
        for row in &cells {
            line(row)?;
        }
        Ok(())
    }

    /// One object per row, then a terminal `{"outcome": ...}` frame.
    fn jsonl(&self) -> Vec<Value> {
        let names: Vec<&str> = self.column_names().collect();
        self.rows
            .iter()
            .map(|row| {
                Value::Object(
                    names
                        .iter()
                        .zip(row)
                        .map(|(name, value)| ((*name).to_owned(), value.clone()))
                        .collect(),
                )
            })
            .chain([json!({ "outcome": self.outcome })])
            .collect()
    }
}

impl QueryReport {
    fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns
            .iter()
            .map(|column| column["name"].as_str().unwrap_or("?"))
    }

    fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            outcome: Value::Null,
        }
    }
}

impl Command for Query<'_> {
    type Output = QueryReport;

    #[allow(clippy::print_stderr, reason = "the outcome summary is status output")]
    fn run(&self, cx: &CommandContext) -> anyhow::Result<Outcome<QueryReport>> {
        let sql = if self.args.sql == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            text
        } else {
            self.args.sql.clone()
        };
        let root = project_root(cx.project_dir.as_deref())?;
        match self.cloud_session(&root)? {
            Some(session) => {
                if matches!(self.backend, Backend::Auto) {
                    eprintln!(
                        "note: querying Boundary Cloud; `baml local recordings query` queries this project's local recordings"
                    );
                }
                self.run_cloud(session, sql)
            }
            None => {
                if matches!(self.backend, Backend::Auto) {
                    eprintln!(
                        "note: querying local recordings; `baml cloud auth login` to query Boundary Cloud"
                    );
                }
                self.run_local(&root, sql)
            }
        }
    }
}

struct CloudSession {
    endpoint: bcs_api::Endpoint,
    credential: bcs_api::Secret,
    source: CredentialSource,
    target: Target,
}

impl Query<'_> {
    /// The cloud session this query runs against, or `None` for local.
    fn cloud_session(&self, root: &Path) -> anyhow::Result<Option<CloudSession>> {
        let scope = match &self.backend {
            Backend::Local => return Ok(None),
            Backend::Cloud(scope) => Some(*scope),
            Backend::Auto => None,
        };
        let api_key = baml_env::string_var("BOUNDARY_API_KEY")?;
        if api_key.as_deref() == Some(bcs_api::credentials::LOCAL_API_KEY) {
            anyhow::ensure!(
                scope.is_none(),
                "BOUNDARY_API_KEY=local selects local recordings; unset it to query Boundary Cloud"
            );
            return Ok(None);
        }
        let settings = crate::cloud_config::Boundary::read(root)?;
        let source = if api_key.is_some() {
            CredentialSource::ApiKey
        } else {
            CredentialSource::SavedLogin
        };
        let failed = |endpoint: Option<bcs_api::Endpoint>, source, error| {
            DiagnosticContext {
                endpoint,
                source,
                operation: Operation::Query,
            }
            .report(error, Failure::QueryFailed)
        };
        let endpoint = settings
            .endpoint()
            .map_err(|error| failed(None, CredentialSource::Configured, error))?;
        // TOML is trusted endpoint configuration: API keys authenticate
        // against it without requiring a separate environment override.
        let credential = match api_key {
            Some(key) => Some(bcs_api::Secret::new(key)),
            // Only confirmed absence may select local recordings. An unreadable
            // credential store must not silently change the query's data source.
            None => bcs_api::Store::new(&endpoint)
                .and_then(|store| store.read())
                .map_err(|error| failed(Some(endpoint.clone()), source, error))?
                .map(|session| session.refresh_token),
        };
        let Some(credential) = credential else {
            if scope.is_none() {
                return Ok(None);
            }
            return Err(failed(Some(endpoint), source, bcs_api::Error::MissingCredentials).into());
        };
        let project = match scope.and_then(|scope| scope.project.clone()) {
            Some(project) => Some(project),
            None => baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project),
        };
        Ok(Some(CloudSession {
            endpoint,
            credential,
            source,
            target: Target {
                project,
                environment: scope.and_then(|scope| scope.env.clone()),
                ..Target::default()
            },
        }))
    }

    #[allow(clippy::print_stderr, reason = "the outcome summary is status output")]
    fn run_local(&self, root: &Path, sql: String) -> anyhow::Result<Outcome<QueryReport>> {
        let request = QueryRequest {
            sql,
            params: Vec::new(),
            budgets: Budgets {
                max_rows: self.args.max_rows,
                max_duration: self.args.max_wall.or(Budgets::default().max_duration),
                ..Budgets::default()
            },
            explain: false,
        };
        let result = match Index::for_project(root, IndexOptions::default())
            .and_then(|mut index| index.refresh_and_query(&request))
        {
            Ok(result) => result,
            Err(error) => {
                crate::reporter::print_error(&error);
                let status = match error {
                    Error::Sql(_) => ExitCode::QueryInvalid,
                    Error::Budget(_) => ExitCode::QueryBudgetExhausted,
                    _ => ExitCode::QueryFailed,
                };
                return Ok(Outcome::new(QueryReport::empty(), status));
            }
        };
        let outcome = &result.outcome;
        let status = match outcome.status {
            Status::Complete => ExitCode::Success,
            Status::Incomplete => ExitCode::QueryIncomplete,
            Status::Truncated => ExitCode::QueryBudgetExhausted,
        };
        let indexed: i64 = outcome.recordings.iter().map(|r| r.indexed_sequence).sum();
        eprintln!(
            "{} row(s); {}; {} recording(s), {indexed} file(s) indexed{}",
            result.rows.len(),
            match outcome.status {
                Status::Complete => "complete",
                Status::Incomplete => "incomplete: some evidence was unavailable",
                Status::Truncated => "truncated by a budget",
            },
            outcome.recordings.len(),
            if outcome.unsealed {
                "; unsealed: the prefix may grow"
            } else {
                ""
            },
        );
        for diagnostic in &outcome.diagnostics {
            crate::reporter::print_warning(format_args!(
                "[{}] x{}: {}",
                diagnostic.code, diagnostic.count, diagnostic.message
            ));
        }
        if outcome.source_missing {
            eprintln!(
                "note: no recordings at {}; run BAML code in this project to record some",
                root.join(".baml/btel/recordings").display()
            );
        }
        let report = QueryReport {
            columns: result
                .columns
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()?,
            outcome: serde_json::to_value(&result.outcome)?,
            rows: result.rows,
        };
        Ok(Outcome::new(report, status))
    }

    #[allow(clippy::print_stderr, reason = "the outcome summary is status output")]
    fn run_cloud(
        &self,
        session: CloudSession,
        sql: String,
    ) -> anyhow::Result<Outcome<QueryReport>> {
        let diagnostic_context = DiagnosticContext {
            endpoint: Some(session.endpoint.clone()),
            source: session.source,
            operation: Operation::Query,
        };
        let authorization = RequestAuthorization {
            authentication: Authentication::shared(session.endpoint.as_str(), session.credential),
            target: Some(session.target),
        };
        let client = bcs_api::Client::new(session.endpoint)
            .map_err(|error| diagnostic_context.report(error, Failure::QueryFailed))?;
        let query_id = uuid::Uuid::new_v4().simple().to_string();
        let budgets = bcs_api::query::Budgets {
            max_rows: self.args.max_rows,
            max_wall_ms: self
                .args
                .max_wall
                .map(|duration| u64::try_from(duration.as_millis()))
                .transpose()
                .context("query wall-clock budget is too large")?,
        };
        let mut report = QueryReport::empty();
        client
            .query(&authorization, &query_id, &sql, budgets, |record| {
                match record {
                    Record::Columns(value) => report.columns = value,
                    Record::Row(value) => report.rows.push(value),
                    Record::QueryOutcome(value) => report.outcome = value,
                }
                Ok(())
            })
            .map_err(|error| diagnostic_context.report(error, Failure::QueryFailed))?;

        eprintln!("{} row(s)", report.rows.len());
        let status = match report.outcome["status"].as_str() {
            Some("complete") => ExitCode::Success,
            Some("incomplete") => {
                crate::reporter::print_warning(
                    "Cloud query results are incomplete because some requested data was unavailable.",
                );
                ExitCode::QueryIncomplete
            }
            Some("truncated") => ExitCode::QueryBudgetExhausted,
            Some("failed") => {
                let error = &report.outcome["error"];
                match (error["code"].as_str(), error["message"].as_str()) {
                    (Some(code), Some(message)) => {
                        crate::reporter::print_error(format_args!(
                            "Cloud query failed ({}): {}",
                            code.escape_debug(),
                            message.escape_debug()
                        ));
                    }
                    (None, Some(message)) => crate::reporter::print_error(format_args!(
                        "Cloud query failed: {}",
                        message.escape_debug()
                    )),
                    _ => crate::reporter::print_error(format_args!("Cloud query failed: {error}")),
                }
                ExitCode::QueryFailed
            }
            Some("cancelled") => ExitCode::QueryCancelled,
            _ => anyhow::bail!("Boundary returned an unknown query outcome"),
        };
        Ok(Outcome::new(report, status))
    }
}

/// The relations a query can read.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct SchemaReport {
    relations: Vec<&'static catalog::Relation>,
}

impl Report for SchemaReport {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        for relation in &self.relations {
            writeln!(out, "{}: {}", relation.name, relation.doc)?;
            for column in relation.columns {
                let doc = if column.doc.is_empty() {
                    String::new()
                } else {
                    format!("  {}", column.doc)
                };
                writeln!(out, "  {:<26} {:<11}{doc}", column.name, column.sql_type)?;
            }
            writeln!(out)?;
        }
        if self.relations.len() == catalog::RELATIONS.len() {
            writeln!(out, "BAML value functions:")?;
            writeln!(
                out,
                "  baml_value_state(v)  present, null, missing, omitted, redacted, no_value, or why the value is unavailable (cas_missing, value_truncated, capture_pending, ...)"
            )?;
            writeln!(
                out,
                "  baml_kind(v)         null, bool, int, float, string, bigint, enum, json (structured), redacted, missing, unavailable"
            )?;
            writeln!(
                out,
                "  baml_code(v)         BAML code that rebuilds a recorded type, or an instance's or enum value's type, classes made at run time included; NULL for other values"
            )?;
        }
        Ok(())
    }
}

impl Command for SchemaArgs {
    type Output = SchemaReport;

    fn run(&self, _cx: &CommandContext) -> anyhow::Result<Outcome<SchemaReport>> {
        let relations: Vec<&catalog::Relation> = catalog::RELATIONS
            .iter()
            .filter(|relation| {
                self.table
                    .as_deref()
                    .is_none_or(|table| relation.name.eq_ignore_ascii_case(table))
            })
            .collect();
        if relations.is_empty() {
            let table = self.table.as_deref().unwrap_or_default();
            anyhow::bail!("unknown relation `{table}`");
        }
        Ok(Outcome::ok(SchemaReport { relations }))
    }
}

fn project_root(project_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    Ok(match find_project_root_from(project_dir)? {
        Some(root) => root,
        None => match project_dir {
            Some(dir) => dir.to_path_buf(),
            None => std::env::current_dir()?,
        },
    })
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
