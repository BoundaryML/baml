//! `baml query` — the portable Project Studio SQL surface (Q2, design
//! 06-studio-experience "Target CLI").
//!
//! Output contract: data rows to stdout; diagnostics and the human
//! terminal outcome to stderr; `--format json` emits one versioned
//! envelope; `--format jsonl` emits one JSON object per row plus a
//! mandatory terminal control frame. A stream without its outcome is
//! never a successful complete result (D13).

use std::path::PathBuf;

use baml_query::outcome::{QueryOutcome, ResultState};
use clap::{Parser, ValueEnum};
use datafusion::arrow::array::Array as _;
use datafusion::arrow::record_batch::RecordBatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryFormat {
    Table,
    Json,
    Jsonl,
}

#[derive(Debug, Parser)]
#[command(
    after_help = "Exit codes: 0 complete; 1 completed but evidence-incomplete \
                  (see the outcome's valueEvaluations); 2 invalid SQL or \
                  authorization; 3 query budget exceeded; 4 cancelled; \
                  5 internal or dependency failure.\n\nExamples:\n  baml query \
                  --schema\n  baml query \"SELECT run_id, status, total_errors \
                  FROM runs_v1 ORDER BY started_at DESC LIMIT 10\"\n  baml query \
                  \"SELECT call_id FROM retained_calls_v1 WHERE \
                  args['customer']['age'] >= 30\" --format jsonl"
)]
pub struct QueryArgs {
    /// Portable SQL against the versioned logical catalog (see --schema).
    pub sql: Option<String>,
    /// Print the catalog schema (relations, columns, types, docs).
    #[arg(long)]
    pub schema: bool,
    /// Restrict --schema output to one relation.
    #[arg(long, value_name = "RELATION")]
    pub view: Option<String>,
    #[arg(long, value_enum, default_value_t = QueryFormat::Table)]
    pub format: QueryFormat,
    /// Project directory (defaults to the current directory's project).
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,
}

impl QueryArgs {
    pub fn run(&self) -> i32 {
        if self.schema {
            return self.print_schema();
        }
        let Some(sql) = &self.sql else {
            eprintln!("error: provide SQL or --schema");
            return 2;
        };
        let baml_dir = match self.baml_dir() {
            Ok(dir) => dir,
            Err(message) => {
                eprintln!("error: {message}");
                return 5;
            }
        };
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                eprintln!("error: {err}");
                return 5;
            }
        };
        runtime.block_on(self.run_sql(&baml_dir, sql))
    }

    fn baml_dir(&self) -> Result<PathBuf, String> {
        let start = self.from.clone().unwrap_or_else(|| PathBuf::from("."));
        let root = bex_events::history::path::resolve_project_root(&start);
        let dir = root.join(".baml");
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(format!(
                "no .baml directory under {} — run something first (`baml run …`)",
                root.display()
            ))
        }
    }

    async fn run_sql(&self, baml_dir: &std::path::Path, sql: &str) -> i32 {
        let session = match baml_query_local::local_session(baml_dir) {
            Ok(session) => session,
            Err(err) => {
                eprintln!("error: {err}");
                return 5;
            }
        };
        match session.execute(sql).await {
            Ok(mut execution) => {
                let mut batches = Vec::new();
                while let Some(batch) = execution.next_batch().await {
                    batches.push(batch);
                }
                let outcome = execution.finish();
                self.emit(&batches, &outcome);
                exit_code(&outcome)
            }
            Err((err, outcome)) => {
                self.emit(&[], &outcome);
                eprintln!("error: {err}");
                exit_code(&outcome)
            }
        }
    }

    fn emit(&self, batches: &[RecordBatch], outcome: &QueryOutcome) {
        match self.format {
            QueryFormat::Table => {
                render_table(batches);
                eprintln!(
                    "-- {} row(s); {}",
                    outcome.rows_streamed,
                    outcome_line(outcome)
                );
            }
            QueryFormat::Json => {
                let rows: Vec<serde_json::Value> = batches.iter().flat_map(batch_rows).collect();
                let envelope = serde_json::json!({
                    "version": "v1",
                    "rows": rows,
                    "queryOutcome": outcome,
                });
                println!("{envelope}");
            }
            QueryFormat::Jsonl => {
                for batch in batches {
                    for row in batch_rows(batch) {
                        println!("{row}");
                    }
                }
                // The mandatory terminal control frame.
                println!("{}", serde_json::json!({ "queryOutcome": outcome }));
            }
        }
    }

    fn print_schema(&self) -> i32 {
        let catalog = baml_query::catalog::catalog_v1();
        let relations: Vec<serde_json::Value> = catalog
            .relations
            .iter()
            .filter(|r| {
                self.view
                    .as_deref()
                    .is_none_or(|v| r.name == v || r.alias == v)
            })
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "alias": r.alias,
                    "provisional": r.provisional,
                    "doc": r.doc,
                    "columns": r.columns.iter().map(|c| serde_json::json!({
                        "name": c.name,
                        "type": format!("{:?}", c.data_type),
                        "nullable": c.nullable,
                        "key": c.key,
                        "virtual": c.value_role.is_some(),
                        "doc": c.doc,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        if relations.is_empty() {
            eprintln!(
                "error: unknown relation {:?}",
                self.view.as_deref().unwrap_or("")
            );
            return 2;
        }
        println!(
            "{}",
            serde_json::json!({ "catalogVersion": "v1", "relations": relations })
        );
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

fn outcome_line(outcome: &QueryOutcome) -> String {
    let values = &outcome.value_evaluations;
    let mut line = format!(
        "result={:?} generation={}",
        outcome.result_state, outcome.snapshot.generation
    );
    if values.attempted > 0 {
        line.push_str(&format!(
            " values={}/{} available",
            values.available, values.attempted
        ));
    }
    line
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
        BinaryArray, BooleanArray, FixedSizeBinaryArray, FixedSizeListArray, Float64Array,
        Int64Array, ListArray, StringArray, TimestampNanosecondArray, UInt32Array, UInt64Array,
    };
    use serde_json::json;
    if column.is_null(row) {
        return serde_json::Value::Null;
    }
    let any = column.as_any();
    if let Some(a) = any.downcast_ref::<StringArray>() {
        // Rendered virtual values are JSON for structured leaves; keep
        // them as text (the caller knows the column's meaning).
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
        return json!(a.value(row));
    }
    if let Some(a) = any.downcast_ref::<FixedSizeBinaryArray>() {
        return json!(hex(a.value(row)));
    }
    if let Some(a) = any.downcast_ref::<BinaryArray>() {
        return json!(hex(a.value(row)));
    }
    if let Some(a) = any.downcast_ref::<ListArray>() {
        let inner = a.value(row);
        return serde_json::Value::Array((0..inner.len()).map(|j| cell_json(&inner, j)).collect());
    }
    if let Some(a) = any.downcast_ref::<FixedSizeListArray>() {
        let inner = a.value(row);
        return serde_json::Value::Array((0..inner.len()).map(|j| cell_json(&inner, j)).collect());
    }
    json!(format!("<{}>", column.data_type()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn render_table(batches: &[RecordBatch]) {
    let Some(first) = batches.first() else {
        return;
    };
    let names: Vec<String> = first
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for batch in batches {
        for i in 0..batch.num_rows() {
            rows.push(
                batch
                    .columns()
                    .iter()
                    .map(|c| match cell_json(c, i) {
                        serde_json::Value::Null => String::new(),
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect(),
            );
        }
    }
    let mut widths: Vec<usize> = names.iter().map(String::len).collect();
    for row in &rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.len().min(60));
        }
    }
    let print_row = |cells: &[String]| {
        let line: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{:<width$}", truncate(c, 60), width = w))
            .collect();
        println!("{}", line.join("  "));
    };
    print_row(&names);
    for row in &rows {
        print_row(row);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}
