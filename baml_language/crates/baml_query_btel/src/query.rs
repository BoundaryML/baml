//! Executing translated SQL in a fixed read snapshot.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use btel_reader::value::Kind;
use rusqlite::{
    Connection,
    hooks::{AuthAction, AuthContext, Authorization},
    types::ValueRef,
};
use serde::Serialize;
use serde_json::Value as Json;

use crate::{
    Error,
    functions::{ContextSlot, QueryContext, ValueMetrics},
    ingest::RefreshMetrics,
    sql::{self, KIND_SUFFIX, OutputColumn},
};

#[derive(Clone, Debug)]
pub struct Budgets {
    pub max_rows: Option<u64>,
    /// Approximate serialized bytes of returned cells.
    pub max_output_bytes: Option<u64>,
    /// Wall-clock bound for SQL execution, including value callbacks. It
    /// also bounds how long a read transaction delays WAL checkpoints.
    pub max_duration: Option<Duration>,
}
impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_rows: None,
            max_output_bytes: Some(64 << 20),
            max_duration: Some(Duration::from_secs(60)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct QueryRequest {
    pub sql: String,
    /// Positional parameters for `?`/`?N` placeholders.
    pub params: Vec<rusqlite::types::Value>,
    pub budgets: Budgets,
    /// Return SQLite's plan for the translated statement instead of rows.
    pub explain: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResultColumn {
    pub name: String,
    /// `baml_value` for navigated or whole BAML values, otherwise `sql`.
    #[serde(rename = "type")]
    pub column_type: &'static str,
}

/// Indexed extent of one recording at the query's read snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct Extent {
    pub recording_id: String,
    pub state: String,
    /// `sealed` once an end marker is indexed, else `unsealed`.
    pub seal_state: String,
    /// `complete_prefix`, `gap`, `invalid_file` or `files_after_end`.
    pub prefix_state: String,
    pub indexed_sequence: i64,
    pub observed_sequence: i64,
    pub terminal_sequence: Option<i64>,
    pub blocked_sequence: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub count: u64,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Every value the query evaluated was available and every recording's
    /// discovered files were indexed.
    Complete,
    /// Rows are correct for the indexed evidence, but some evidence was
    /// unavailable: see diagnostics.
    Incomplete,
    /// A row or output budget stopped the result early.
    Truncated,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QueryMetrics {
    pub translate_ms: f64,
    pub sql_ms: f64,
    pub render_ms: f64,
    pub total_ms: f64,
    pub rows: u64,
    #[serde(flatten)]
    pub values: ValueMetrics,
}

#[derive(Clone, Debug, Serialize)]
pub struct Outcome {
    pub status: Status,
    /// True when some recording has no end marker: its indexed prefix may
    /// grow on the next refresh. Not evidence that anything is still running.
    pub unsealed: bool,
    /// No recording directory exists for this project yet.
    pub source_missing: bool,
    pub recordings: Vec<Extent>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<RefreshMetrics>,
    pub query: QueryMetrics,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Json>>,
    pub outcome: Outcome,
    /// The SQL handed to SQLite, for debugging and `--explain`.
    #[serde(skip)]
    pub translated_sql: String,
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

/// SQLite built-ins a read-only query must not reach.
const DENIED_FUNCTIONS: &[&str] = &[
    "load_extension",
    "readfile",
    "writefile",
    "edit",
    "fts3_tokenizer",
];

fn authorize(context: AuthContext<'_>) -> Authorization {
    match context.action {
        AuthAction::Select | AuthAction::Read { .. } | AuthAction::Recursive => {
            Authorization::Allow
        }
        AuthAction::Function { function_name } => {
            if DENIED_FUNCTIONS
                .iter()
                .any(|denied| function_name.eq_ignore_ascii_case(denied))
            {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        _ => Authorization::Deny,
    }
}

fn cell(value: ValueRef<'_>) -> Json {
    match value {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(n) => Json::from(n),
        ValueRef::Real(f) => {
            serde_json::Number::from_f64(f).map_or_else(|| Json::from(f.to_string()), Json::Number)
        }
        ValueRef::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => {
            use base64::Engine as _;
            let mut map = serde_json::Map::new();
            map.insert(
                "$blob".into(),
                Json::from(base64::engine::general_purpose::STANDARD.encode(b)),
            );
            Json::Object(map)
        }
    }
}

/// Restore the BAML kind of a rendered value cell for typed output.
fn typed(rendered: Json, kind: Option<&str>) -> Json {
    match kind.and_then(Kind::from_label) {
        Some(Kind::Bool) => Json::Bool(rendered.as_i64().is_some_and(|n| n != 0)),
        Some(Kind::Json) => rendered
            .as_str()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(rendered),
        Some(Kind::Bigint) => {
            let mut map = serde_json::Map::new();
            map.insert("$bigint".into(), rendered);
            Json::Object(map)
        }
        Some(Kind::Float) if rendered.is_string() => {
            let mut map = serde_json::Map::new();
            map.insert("$float".into(), rendered);
            Json::Object(map)
        }
        _ => rendered,
    }
}

pub(crate) fn read_extents(conn: &Connection) -> Result<Vec<Extent>, Error> {
    let mut statement = conn.prepare(
        "SELECT recording_id, state, seal_state, prefix_state, indexed_sequence,
           observed_sequence, terminal_sequence, blocked_sequence
         FROM temp.recordings ORDER BY recording_id",
    )?;
    let rows = statement.query_map([], |r| {
        Ok(Extent {
            recording_id: r.get(0)?,
            state: r.get(1)?,
            seal_state: r.get(2)?,
            prefix_state: r.get(3)?,
            indexed_sequence: r.get(4)?,
            observed_sequence: r.get(5)?,
            terminal_sequence: r.get(6)?,
            blocked_sequence: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn diagnostic_message(code: &str) -> &'static str {
    match code {
        "cas_missing" => "captured value blob not found in the CAS directory",
        "cas_unreadable" => "captured value blob could not be read",
        "cas_too_large" => "captured value blob exceeds the reader's size limit",
        "cas_id_mismatch" => "captured value blob content does not match its id",
        "cas_decode_limit" => "captured value exceeds a decoder limit",
        "cas_unsupported_version" => "captured value uses an unsupported blob format",
        "cas_corrupt" => "captured value blob is invalid",
        "value_truncated" => "the producer's capture limits cut part of this value",
        "argument_names_unknown" => {
            "no parameter names were recorded for this function; use args[0]"
        }
        "argument_layout_mismatch" => {
            "recorded parameter names do not match the captured argument count"
        }
        "capture_root_mismatch" => "captured blob has an unexpected root shape",
        "capture_pending" => {
            "the call completed but the announcement carrying its captured args is not indexed yet"
        }
        "comparison_unsupported" => {
            "this comparison needs unsupported value semantics: ordering structured values, or comparing opaque/descriptive/type values"
        }
        "comparison_cycle" => {
            "whole-value comparison of cyclic graphs is not supported; compare a finite path into the value"
        }
        "comparison_limit" => {
            "comparison exceeded its depth, node or byte limit; compare a smaller subtree"
        }
        "value_cycle" => "a cycle of cells does not resolve to a captured logical value",
        _ => "evidence unavailable",
    }
}

/// Run `request` against `conn` inside one read transaction.
pub(crate) fn run(
    conn: &mut Connection,
    slot: &ContextSlot,
    context: QueryContext,
    request: &QueryRequest,
) -> Result<QueryResult, Error> {
    let started = Instant::now();
    let translation = sql::translate(&request.sql).map_err(Error::Sql)?;
    let translate_ms = ms(started.elapsed());
    let deadline = request.budgets.max_duration.map(|d| Instant::now() + d);

    let context = Arc::new(context);
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    // The first read fixes the snapshot; extents and rows share it.
    let recordings = read_extents(&tx)?;
    *slot.lock().expect("context slot") = Some(Arc::clone(&context));
    let result = (|| -> Result<Collected, Error> {
        let sql_started = Instant::now();
        let text = if request.explain {
            format!("EXPLAIN QUERY PLAN {}", translation.sql)
        } else {
            translation.sql.clone()
        };
        tx.authorizer(Some(authorize));
        if let Some(deadline) = deadline {
            tx.progress_handler(10_000, Some(move || Instant::now() >= deadline));
        }
        let prepared = tx.prepare(&text);
        tx.authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let mut statement = prepared.map_err(|error| match error {
            rusqlite::Error::SqliteFailure(_, Some(message)) => Error::Sql(sql::SqlError(message)),
            other => Error::from(other),
        })?;
        let names: Vec<String> = statement
            .column_names()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let columns: Vec<OutputColumn> = if request.explain {
            names
                .iter()
                .map(|name| OutputColumn {
                    name: name.clone(),
                    value: false,
                })
                .collect()
        } else {
            translation.columns.clone()
        };
        // Physical positions: a value column is followed by its kind column.
        let mut positions = Vec::new();
        let mut physical = 0;
        for column in &columns {
            let kind_at = column.value.then_some(physical + 1);
            if let Some(kind) = kind_at {
                debug_assert!(names.get(kind).is_some_and(|n| n.ends_with(KIND_SUFFIX)));
            }
            positions.push((physical, kind_at));
            physical += if column.value { 2 } else { 1 };
        }
        if physical != names.len() {
            // Output metadata and SQLite disagree (e.g. an unusual form):
            // fall back to plain columns.
            positions = (0..names.len()).map(|i| (i, None)).collect();
        }
        let result_columns: Vec<ResultColumn> = if physical == names.len() {
            columns
                .iter()
                .map(|c| ResultColumn {
                    name: c.name.clone(),
                    column_type: if c.value { "baml_value" } else { "sql" },
                })
                .collect()
        } else {
            names
                .iter()
                .map(|name| ResultColumn {
                    name: name.clone(),
                    column_type: "sql",
                })
                .collect()
        };
        let mut rows = Vec::new();
        let mut truncated = false;
        let mut output_bytes = 0_u64;
        let params = rusqlite::params_from_iter(request.params.iter());
        let mut cursor = statement.query(params).map_err(map_budget)?;
        loop {
            let Some(row) = cursor.next().map_err(map_budget)? else {
                break;
            };
            if request
                .budgets
                .max_rows
                .is_some_and(|max| rows.len() as u64 >= max)
            {
                truncated = true;
                break;
            }
            let mut values = Vec::with_capacity(positions.len());
            for (at, kind_at) in &positions {
                let rendered = cell(row.get_ref(*at)?);
                let value = match kind_at {
                    Some(kind_at) => {
                        let kind = row.get_ref(*kind_at)?;
                        let kind = match kind {
                            ValueRef::Text(t) => std::str::from_utf8(t).ok(),
                            _ => None,
                        };
                        typed(rendered, kind)
                    }
                    None => rendered,
                };
                output_bytes += approximate_size(&value);
                values.push(value);
            }
            rows.push(values);
            if request
                .budgets
                .max_output_bytes
                .is_some_and(|max| output_bytes > max)
            {
                truncated = true;
                break;
            }
        }
        drop(cursor);
        Ok(Collected {
            columns: result_columns,
            rows,
            truncated,
            sql_ms: ms(sql_started.elapsed()),
        })
    })();
    *slot.lock().expect("context slot") = None;
    tx.progress_handler(0, None::<fn() -> bool>);
    let Collected {
        columns,
        rows,
        truncated,
        sql_ms,
    } = result?;
    tx.commit()?;

    let values = context.metrics();
    let mut diagnostics: Vec<Diagnostic> = values
        .unavailable
        .iter()
        .map(|(code, count)| Diagnostic {
            code: code.clone(),
            count: *count,
            message: diagnostic_message(code).to_owned(),
        })
        .collect();
    for extent in &recordings {
        if let Some(blocked) = extent.blocked_sequence {
            diagnostics.push(Diagnostic {
                code: if extent.prefix_state == "invalid_file" {
                    "recording_invalid_file".into()
                } else {
                    "recording_gap".into()
                },
                count: 1,
                message: format!(
                    "recording {} is indexed through file {}; file {blocked} stops indexing (see the issues relation)",
                    extent.recording_id, extent.indexed_sequence
                ),
            });
        }
    }
    let status = if truncated {
        Status::Truncated
    } else if diagnostics.is_empty() {
        Status::Complete
    } else {
        Status::Incomplete
    };
    let unsealed = recordings.iter().any(|r| r.seal_state != "sealed");
    let rows_count = rows.len() as u64;
    Ok(QueryResult {
        columns,
        rows,
        translated_sql: translation.sql,
        outcome: Outcome {
            status,
            unsealed,
            source_missing: false,
            recordings,
            diagnostics,
            refresh: None,
            query: QueryMetrics {
                translate_ms,
                sql_ms,
                render_ms: 0.0,
                total_ms: ms(started.elapsed()),
                rows: rows_count,
                values,
            },
        },
    })
}

struct Collected {
    columns: Vec<ResultColumn>,
    rows: Vec<Vec<Json>>,
    truncated: bool,
    sql_ms: f64,
}

fn approximate_size(value: &Json) -> u64 {
    match value {
        Json::Null | Json::Bool(_) => 4,
        Json::Number(_) => 8,
        Json::String(s) => s.len() as u64 + 2,
        other => other.to_string().len() as u64,
    }
}

fn map_budget(error: rusqlite::Error) -> Error {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            Error::Budget("query time budget exceeded".into())
        }
        rusqlite::Error::UserFunctionError(inner) if inner.to_string().contains("budget") => {
            Error::Budget(inner.to_string())
        }
        rusqlite::Error::SqliteFailure(_, Some(message)) if message.contains("budget") => {
            Error::Budget(message.clone())
        }
        _ => Error::from(error),
    }
}
