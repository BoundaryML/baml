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
/// `SELECT output_value['items'][0]['name'] FROM spans WHERE input_args['customer']['age'] >= 30`.
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

    /// Query this project's local recordings explicitly.
    #[arg(long)]
    pub local: bool,

    /// Boundary cloud project in org_handle/project_name form.
    #[arg(long, id = "boundary_project", conflicts_with = "local")]
    pub project: Option<String>,

    /// Boundary cloud environment; defaults to your personal environment.
    #[arg(long, conflicts_with = "local")]
    pub environment: Option<String>,

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

/// Value functions, after the relations.
fn print_guide() {
    println!("BAML value functions:");
    println!(
        "  baml_value_state(v)  present, null, missing, omitted, no_value, or why the value is unavailable (cas_missing, value_truncated, capture_pending, ...)"
    );
    println!(
        "  baml_kind(v)         null, bool, int, float, string, bigint, enum, json (structured), missing, unavailable"
    );
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

    fn run_cloud(
        &self,
        endpoint: bcs_api::Endpoint,
        credential: bcs_api::Secret,
        source: bcs_api::diagnostics::CredentialSource,
        project: Option<String>,
        sql: String,
    ) -> anyhow::Result<crate::ExitCode> {
        crate::cloud_query::run(self, endpoint, credential, source, project, sql)
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
        let api_key = if self.local {
            None
        } else {
            baml_env::string_var("BOUNDARY_API_KEY")?
        };
        let local_from_env = api_key.as_deref() == Some(bcs_api::credentials::LOCAL_API_KEY);
        anyhow::ensure!(
            !local_from_env || (self.project.is_none() && self.environment.is_none()),
            "BOUNDARY_API_KEY=local selects local recordings; unset BOUNDARY_API_KEY to use --project or --environment for a cloud query"
        );
        if !self.local && !local_from_env {
            let settings = crate::cloud_config::Boundary::read(&root)?;
            let endpoint = settings.endpoint().map_err(|error| {
                bcs_api::diagnostics::Context {
                    endpoint: None,
                    source: bcs_api::diagnostics::CredentialSource::Configured,
                    operation: bcs_api::diagnostics::Operation::Query,
                }
                .report(error, bcs_api::diagnostics::Outcome::QueryFailed)
            })?;
            let source = if api_key.is_some() {
                bcs_api::diagnostics::CredentialSource::ApiKey
            } else {
                bcs_api::diagnostics::CredentialSource::SavedLogin
            };
            // TOML is trusted endpoint configuration: API keys authenticate
            // against it without requiring a separate environment override.
            let credential = match api_key {
                Some(key) => Some(bcs_api::Secret::new(key)),
                // Optional saved login must not block local queries on hosts without a keyring.
                // Explicit cloud selection below still requires a credential.
                None => bcs_api::Store::new(&endpoint)
                    .and_then(|store| store.read())
                    .unwrap_or(None)
                    .map(|session| session.refresh_token),
            };
            if credential.is_some() || self.project.is_some() || self.environment.is_some() {
                let credential = credential.ok_or_else(|| {
                    bcs_api::diagnostics::Context {
                        endpoint: Some(endpoint.clone()),
                        source,
                        operation: bcs_api::diagnostics::Operation::Query,
                    }
                    .report(
                        bcs_api::Error::MissingCredentials,
                        bcs_api::diagnostics::Outcome::QueryFailed,
                    )
                })?;
                let project = match self.project.clone() {
                    Some(project) => Some(project),
                    None => baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project),
                };
                return self.run_cloud(endpoint, credential, source, project, sql);
            }
        }
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
