//! `baml query` — portable SQL over the local profile store
//! (TASK/baml-query-scope.md §6).
//!
//! Output contract: data rows on stdout; the human terminal outcome on
//! stderr (`table`), or inline: `json` = one versioned envelope, `jsonl`
//! = one JSON object per row plus a mandatory terminal control frame. A
//! stream without its outcome is never a successful complete result.

// Machine-readable rows go to raw stdout and the outcome to raw stderr
// by contract (§6) — the reporter's formatting layers stay out of the
// data path, exactly like `describe`.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    io::{Read as _, Write as _},
    path::PathBuf,
    time::Duration,
};

use anyhow::Context as _;
use baml_query::{
    budget::{CancellationToken, QueryBudgets},
    catalog::CatalogProfile,
    outcome::{QueryOutcome, ResultState},
};
use clap::{Parser, ValueEnum};
use datafusion::arrow::{array::Array as _, record_batch::RecordBatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryFormat {
    Table,
    Json,
    Jsonl,
}

/// Query the local profile store with SQL.
#[derive(Debug, Parser)]
#[command(after_long_help = "Exit codes: 0 complete; 1 completed but \
evidence-incomplete (see the outcome's valueEvaluations); 2 invalid SQL, \
unknown table, or authorization; 3 query budget exceeded; 4 cancelled; \
5 internal or dependency failure (no store, bind failure, corrupt \
artifact).\n\nExamples:\n  baml query \"SHOW TABLES\"\n  baml query \
\"SELECT thread_id, status, total_errors, started_at FROM threads WHERE \
parent_thread_id IS NULL ORDER BY started_at DESC LIMIT 20\"\n  baml query \
\"SELECT fqn, sum(calls_started) calls, sum(self_ns) self_ns FROM call_path_stats \
GROUP BY fqn ORDER BY self_ns DESC LIMIT 10\"\n  baml query \"SELECT \
call_id, args['customer']['age'] AS age, output FROM calls WHERE \
args['customer']['age'] >= 30 LIMIT 50\" --format jsonl\n  baml query \
--schema --table calls")]
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
    fn profile(&self) -> CatalogProfile {
        if self.internal || std::env::var_os("BAML_INTERNAL").is_some_and(|v| v != "0") {
            CatalogProfile::internal()
        } else {
            CatalogProfile::public()
        }
    }

    pub fn run(&self) -> anyhow::Result<crate::ExitCode> {
        if self.schema {
            let code = self.print_schema();
            std::process::exit(code);
        }
        let sql = match self.sql.as_deref() {
            Some("-") => {
                let mut sql = String::new();
                std::io::stdin()
                    .read_to_string(&mut sql)
                    .context("failed to read SQL from stdin")?;
                sql
            }
            Some(sql) => sql.to_string(),
            None => {
                eprintln!("error: provide SQL (or `-` for stdin), or use --schema");
                std::process::exit(2);
            }
        };
        let sql = if self.explain && !sql.trim_start().to_ascii_lowercase().starts_with("explain") {
            format!("EXPLAIN {sql}")
        } else {
            sql
        };
        let project_root = crate::project_load::find_project_root_from(self.from.as_deref())?
            .unwrap_or(std::env::current_dir().context("failed to resolve current directory")?);
        // Same resolution rule as the producer: BAML_PROFILE_DIR wins,
        // else the project store.
        let store_root =
            bex_events::prof::backend::ProfilerSession::resolve_store_root(&project_root);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to start the query runtime")?;
        let code = runtime.block_on(self.run_sql(&store_root, &sql));
        std::process::exit(code);
    }

    async fn run_sql(&self, store_root: &std::path::Path, sql: &str) -> i32 {
        let mut budgets = QueryBudgets::unlimited();
        if let Some(max_rows) = self.max_rows {
            budgets.max_result_rows = max_rows;
        }
        budgets.max_wall = self.max_wall;
        // Ctrl-C is handled by the CLI's global handler (exit 130); the
        // token stays for host-driven cancellation.
        let cancel = CancellationToken::new();
        let session = match baml_query_profiles::profiles_session_with(
            store_root,
            self.profile(),
            budgets,
            cancel,
        )
        .await
        {
            Ok(session) => session,
            Err(err) => {
                eprintln!("error: {err}");
                return 5;
            }
        };
        match session.execute(sql).await {
            Ok(mut execution) => {
                let mut sink = RowSink::new(self.format);
                while let Some(batch) = execution.next_batch().await {
                    sink.batch(&batch);
                }
                let outcome = execution.finish();
                sink.finish(&outcome);
                exit_code(&outcome)
            }
            Err((err, outcome)) => {
                RowSink::new(self.format).finish(&outcome);
                eprintln!("error: {err}");
                exit_code(&outcome)
            }
        }
    }

    fn print_schema(&self) -> i32 {
        let profile = self.profile();
        let relations: Vec<serde_json::Value> = profile
            .relations()
            .iter()
            .filter(|r| {
                self.table
                    .as_deref()
                    .is_none_or(|t| r.names().any(|n| n == t))
            })
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "alias": r.alias,
                    "grain": format!("{:?}", r.grain),
                    "provisional": r.provisional,
                    "doc": r.doc,
                    "columns": r.columns.iter().map(|c| serde_json::json!({
                        "name": c.name,
                        "type": format!("{:?}", c.data_type),
                        "nullable": c.nullable,
                        "key": c.key,
                        "virtual": c.value_role.is_some(),
                        "role": c.value_role,
                        "doc": c.doc,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let views: Vec<serde_json::Value> = profile
            .views()
            .iter()
            .filter(|v| {
                self.table
                    .as_deref()
                    .is_none_or(|t| v.name == t || v.alias == t)
            })
            .map(|v| {
                serde_json::json!({
                    "name": v.name,
                    "alias": v.alias,
                    "sql": v.sql,
                    "doc": v.doc,
                })
            })
            .collect();
        if relations.is_empty() && views.is_empty() {
            eprintln!(
                "error: unknown relation {:?}",
                self.table.as_deref().unwrap_or("")
            );
            return 2;
        }
        match self.format {
            QueryFormat::Table => {
                for relation in &relations {
                    println!(
                        "{} (alias {})",
                        relation["name"].as_str().unwrap_or_default(),
                        relation["alias"].as_str().unwrap_or_default(),
                    );
                    if let Some(columns) = relation["columns"].as_array() {
                        for column in columns {
                            println!(
                                "  {:<28} {:<28} {}",
                                column["name"].as_str().unwrap_or_default(),
                                format!(
                                    "{}{}{}",
                                    column["type"].as_str().unwrap_or_default(),
                                    if column["nullable"].as_bool() == Some(true) {
                                        "?"
                                    } else {
                                        ""
                                    },
                                    if column["virtual"].as_bool() == Some(true) {
                                        " (virtual)"
                                    } else {
                                        ""
                                    },
                                ),
                                column["doc"].as_str().unwrap_or_default(),
                            );
                        }
                    }
                    println!();
                }
                for view in &views {
                    println!(
                        "view {} (alias {}): {}",
                        view["name"].as_str().unwrap_or_default(),
                        view["alias"].as_str().unwrap_or_default(),
                        view["sql"].as_str().unwrap_or_default(),
                    );
                }
            }
            QueryFormat::Json | QueryFormat::Jsonl => {
                println!(
                    "{}",
                    serde_json::json!({
                        "catalogVersion": baml_query::catalog::CATALOG_V1,
                        "relations": relations,
                        "views": views,
                    })
                );
            }
        }
        0
    }
}

fn exit_code(outcome: &QueryOutcome) -> i32 {
    match outcome.result_state {
        ResultState::Complete => 0,
        ResultState::Incomplete => 1,
        ResultState::Failed => match outcome.error.as_ref().map(|e| e.code.as_str()) {
            Some("invalid_sql" | "authorization_denied") => 2,
            _ => 5,
        },
        ResultState::BudgetExhausted => 3,
        ResultState::Cancelled => 4,
    }
}

/// Streaming row emitter: rows leave as soon as their batch arrives (the
/// table's column widths freeze on the first batch).
struct RowSink {
    format: QueryFormat,
    /// Table: (names, widths); JSON: accumulated rows.
    table_layout: Option<(Vec<String>, Vec<usize>)>,
    json_rows: Vec<serde_json::Value>,
    rows_emitted: bool,
}

const CELL_LIMIT: usize = 60;

impl RowSink {
    fn new(format: QueryFormat) -> RowSink {
        RowSink {
            format,
            table_layout: None,
            json_rows: Vec::new(),
            rows_emitted: false,
        }
    }

    fn batch(&mut self, batch: &RecordBatch) {
        self.rows_emitted = true;
        match self.format {
            QueryFormat::Table => self.table_batch(batch),
            QueryFormat::Json => self.json_rows.extend(batch_rows(batch)),
            QueryFormat::Jsonl => {
                let stdout = std::io::stdout();
                let mut out = stdout.lock();
                for row in batch_rows(batch) {
                    let _ = writeln!(out, "{row}");
                }
            }
        }
    }

    fn table_batch(&mut self, batch: &RecordBatch) {
        let (names, widths) = self.table_layout.get_or_insert_with(|| {
            let names: Vec<String> = batch
                .schema()
                .fields()
                .iter()
                .map(|f| f.name().clone())
                .collect();
            let mut widths: Vec<usize> = names.iter().map(String::len).collect();
            for i in 0..batch.num_rows() {
                for (width, column) in widths.iter_mut().zip(batch.columns()) {
                    *width = (*width).max(cell_text(column, i).len().min(CELL_LIMIT));
                }
            }
            let header: Vec<String> = names.clone();
            print_table_row(&header, &widths);
            (names, widths)
        });
        let _ = names;
        for i in 0..batch.num_rows() {
            let cells: Vec<String> = batch
                .columns()
                .iter()
                .map(|column| cell_text(column, i))
                .collect();
            print_table_row(&cells, widths);
        }
    }

    fn finish(mut self, outcome: &QueryOutcome) {
        match self.format {
            QueryFormat::Table => {
                eprintln!(
                    "-- {} row(s); {}",
                    outcome.rows_streamed,
                    outcome_line(outcome)
                );
            }
            QueryFormat::Json => {
                let envelope = serde_json::json!({
                    "version": "v1",
                    "rows": std::mem::take(&mut self.json_rows),
                    "queryOutcome": outcome,
                });
                println!("{envelope}");
            }
            QueryFormat::Jsonl => {
                // The mandatory terminal control frame.
                println!("{}", serde_json::json!({ "queryOutcome": outcome }));
            }
        }
    }
}

fn outcome_line(outcome: &QueryOutcome) -> String {
    let values = &outcome.value_evaluations;
    let mut line = format!(
        "result={} generation={}",
        match outcome.result_state {
            ResultState::Complete => "complete",
            ResultState::Incomplete => "incomplete",
            ResultState::Failed => "failed",
            ResultState::BudgetExhausted => "budget_exhausted",
            ResultState::Cancelled => "cancelled",
        },
        &outcome.snapshot.generation[..12.min(outcome.snapshot.generation.len())],
    );
    if values.attempted > 0 {
        line.push_str(&format!(
            " values={}/{} available",
            values.available, values.attempted
        ));
    }
    line
}

fn print_table_row(cells: &[String], widths: &[usize]) {
    let line: Vec<String> = cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| format!("{:<width$}", truncate(cell, CELL_LIMIT)))
        .collect();
    println!("{}", line.join("  "));
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.saturating_sub(1);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn cell_text(column: &datafusion::arrow::array::ArrayRef, row: usize) -> String {
    if column.is_null(row) {
        return String::new();
    }
    match cell_json(column, row) {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

/// Render one batch's rows as JSON objects (catalog types only).
fn batch_rows(batch: &RecordBatch) -> Vec<serde_json::Value> {
    let mut rows = Vec::with_capacity(batch.num_rows());
    for i in 0..batch.num_rows() {
        let mut object = serde_json::Map::new();
        for (field, column) in batch.schema().fields().iter().zip(batch.columns()) {
            object.insert(field.name().clone(), cell_json(column, i));
        }
        rows.push(serde_json::Value::Object(object));
    }
    rows
}

fn cell_json(column: &datafusion::arrow::array::ArrayRef, row: usize) -> serde_json::Value {
    use datafusion::arrow::array::{
        BinaryArray, BooleanArray, Float64Array, Int64Array, LargeStringArray, ListArray,
        StringArray, StringViewArray, TimestampNanosecondArray, UInt32Array, UInt64Array,
    };
    use serde_json::json;
    if column.is_null(row) {
        return serde_json::Value::Null;
    }
    let any = column.as_any();
    if let Some(a) = any.downcast_ref::<StringArray>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<LargeStringArray>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<StringViewArray>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<UInt64Array>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<UInt32Array>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<Int64Array>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<Float64Array>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<BooleanArray>() {
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<TimestampNanosecondArray>() {
        // RFC 3339 keeps timestamps human- and machine-readable.
        return a
            .value_as_datetime(row)
            .map_or(json!(a.value(row)), |dt| json!(format!("{dt}Z")));
    }
    if let Some(a) = any.downcast_ref::<BinaryArray>() {
        return json!(format!("0x{}", hex_str(a.value(row))));
    }
    if let Some(a) = any.downcast_ref::<ListArray>() {
        let inner = a.value(row);
        return serde_json::Value::Array((0..inner.len()).map(|j| cell_json(&inner, j)).collect());
    }
    // A type outside the catalog surface (EXPLAIN plans etc.): render via
    // Arrow's display.
    datafusion::arrow::util::display::array_value_to_string(column, row)
        .map_or(serde_json::Value::Null, |s| json!(s))
}

fn hex_str(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
