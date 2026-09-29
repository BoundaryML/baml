//! Playground telemetry reads over the project's local Btel recordings.
//!
//! Every read goes through `baml_query_btel`, exactly as `baml query` does:
//! an explicit refresh applies newly completed recording files, then the SQL
//! below runs in a fresh read snapshot with a fresh value cache. The open
//! index is reused across requests. All of it is blocking work: callers run
//! it on a blocking thread, never on the async executor.
//!
//! An execution is a root call: a future span without a parent. It opens
//! with the spans below it, found through `parent_span_id`: its futures
//! (timeline lanes) and its retained calls with their captured values, and
//! the errors those calls ended with. Calling contexts come from the
//! profiler, which exists only once the process that ran the call ended.
//! What recordings do not carry arrives empty or null rather than invented.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, TryLockError},
};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult, SqlParam};
use serde_json::{Value, json};

/// Rows the list shows; the newest root calls first.
const LIST_SQL: &str = "
SELECT a.span_id, a.span_name, s.status, a.start_time, s.duration, p.status
FROM span_announcements a
LEFT JOIN spans s ON s.span_id = a.span_id
LEFT JOIN processes p ON p.process_id = a.process_id
WHERE a.span_type = 'future' AND a.parent_span_id IS NULL
ORDER BY a.start_time DESC
LIMIT 200";

/// Span ids per query: each is a parameter, and the id filter repeats it.
const IDS_PER_QUERY: usize = 256;

fn ids(count: usize) -> String {
    (1..=count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Spans started under any of `count` parents, or those spans themselves.
fn tree_sql(column: &str, count: usize) -> String {
    format!(
        "SELECT span_id, span_type, span_name, parent_span_id, future_id, profiler_node_id,
           start_time, is_complete, process_id
         FROM span_announcements WHERE {column} IN ({})",
        ids(count)
    )
}

/// What completed spans ended with, captured values rendered server-side.
fn completion_sql(count: usize) -> String {
    format!(
        "SELECT span_id, status, end_time, duration, input_args, output_value,
           error_value['error'], baml_value_state(input_args), baml_value_state(output_value),
           baml_value_state(error_value), error_value['stack_trace']['frames']
         FROM spans WHERE span_id IN ({})",
        ids(count)
    )
}

/// Calling contexts of an ended process.
const PROFILER_SQL: &str = "
SELECT profiler_node_id, parent_profiler_node_id, function_name, invocation_count,
  return_count, error_count, cancel_error_count, total_time, self_time, io_total_time
FROM profiler WHERE process_id = ?1";

/// Refused rather than queued: the UI's polling retries it.
pub const BUSY: &str = "telemetryBusy";

#[derive(Debug)]
pub struct TelemetryError {
    pub code: &'static str,
    pub message: String,
}

impl TelemetryError {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            code: "telemetryFailed",
            message: message.into(),
        }
    }
}

fn run(index: &mut Index, sql: &str, params: Vec<SqlParam>) -> Result<QueryResult, TelemetryError> {
    index
        .query(&QueryRequest {
            sql: sql.to_owned(),
            params,
            ..QueryRequest::default()
        })
        .map_err(|e| TelemetryError::failed(e.to_string()))
}

/// `sql(count)` over `ids` in chunks, all rows together.
fn chunked(
    index: &mut Index,
    ids: &[String],
    sql: impl Fn(usize) -> String,
) -> Result<Vec<Vec<Value>>, TelemetryError> {
    let mut rows = Vec::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let params = chunk.iter().cloned().map(SqlParam::Text).collect();
        rows.extend(run(index, &sql(chunk.len()), params)?.rows);
    }
    Ok(rows)
}

pub struct PlaygroundTelemetry {
    workspace_roots: Arc<[PathBuf]>,
    indexes: Mutex<HashMap<PathBuf, Arc<Mutex<Index>>>>,
}

impl PlaygroundTelemetry {
    pub fn new(workspace_roots: Arc<[PathBuf]>) -> Self {
        Self {
            workspace_roots,
            indexes: Mutex::new(HashMap::new()),
        }
    }

    /// Only projects under a served workspace root are readable.
    fn project(&self, project: &str) -> Result<PathBuf, TelemetryError> {
        let path = std::fs::canonicalize(project)
            .map_err(|e| TelemetryError::failed(format!("{project}: {e}")))?;
        let served = self
            .workspace_roots
            .iter()
            .any(|root| std::fs::canonicalize(root).is_ok_and(|root| path.starts_with(root)));
        if !served {
            return Err(TelemetryError::failed(format!(
                "{project} is not a workspace served by this playground"
            )));
        }
        Ok(path)
    }

    fn index(&self, project: &Path) -> Result<Option<Arc<Mutex<Index>>>, TelemetryError> {
        if let Some(index) = self.indexes.lock().expect("index table").get(project) {
            return Ok(Some(Arc::clone(index)));
        }
        // Opening can wait on another process's index lock, so other projects
        // must not wait on the table meanwhile.
        let index = Index::for_project(project, IndexOptions::default())
            .map_err(|e| TelemetryError::failed(e.to_string()))?;
        // Nothing recorded yet: do not cache an empty in-memory index, so the
        // first recording is found by the next request.
        if index.source_missing() {
            return Ok(None);
        }
        // A request that opened the same project meanwhile keeps its index.
        let mut indexes = self.indexes.lock().expect("index table");
        let index = indexes
            .entry(project.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(index)));
        Ok(Some(Arc::clone(index)))
    }

    /// Reconcile new files, then hand the project's index to `read`.
    fn with_index<T>(
        &self,
        project: &str,
        read: impl FnOnce(&mut Index) -> Result<T, TelemetryError>,
    ) -> Result<Option<T>, TelemetryError> {
        let project = self.project(project)?;
        let Some(index) = self.index(&project)? else {
            return Ok(None);
        };
        let mut index = match index.try_lock() {
            Ok(index) => index,
            Err(TryLockError::WouldBlock) => {
                return Err(TelemetryError {
                    code: BUSY,
                    message: "telemetry for this project is already being read".into(),
                });
            }
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        index
            .refresh()
            .map_err(|e| TelemetryError::failed(e.to_string()))?;
        read(&mut index).map(Some)
    }

    /// Execution list and whether the project has no recordings yet.
    pub fn list_executions(&self, project: &str) -> Result<(Vec<Value>, bool), TelemetryError> {
        match self.with_index(project, |index| run(index, LIST_SQL, Vec::new()))? {
            None => Ok((Vec::new(), true)),
            Some(result) => Ok((
                result.rows.iter().map(|row| execution(row)).collect(),
                false,
            )),
        }
    }

    /// `current_source` is the source identity of the program the playground
    /// has built for `project` now; recorded locations are `verified` only
    /// when it equals the recording's.
    pub fn open_execution(
        &self,
        project: &str,
        execution_id: &str,
        current_source: Option<[u8; 32]>,
    ) -> Result<Value, TelemetryError> {
        let Some(opened) = self.with_index(project, |index| {
            // The root and every span below it, one level per query.
            let mut spans = chunked(index, &[execution_id.to_owned()], |n| {
                tree_sql("span_id", n)
            })?;
            let mut frontier = vec![execution_id.to_owned()];
            while !frontier.is_empty() {
                let children = chunked(index, &frontier, |n| tree_sql("parent_span_id", n))?;
                frontier = children
                    .iter()
                    .filter_map(|row| row[0].as_str().map(str::to_owned))
                    .collect();
                spans.extend(children);
            }
            let ids: Vec<String> = spans
                .iter()
                .filter_map(|row| row[0].as_str().map(str::to_owned))
                .collect();
            let completions = chunked(index, &ids, completion_sql)?;
            let root = spans.first().cloned();
            let process = root
                .as_ref()
                .and_then(|root| root[8].as_str().map(str::to_owned));
            let profiler = match &process {
                Some(process) => {
                    run(index, PROFILER_SQL, vec![SqlParam::Text(process.clone())])?.rows
                }
                None => Vec::new(),
            };
            let list = run(
                index,
                &format!("{LIST_SQL_ONE} WHERE a.span_id = ?1"),
                vec![SqlParam::Text(execution_id.to_owned())],
            )?
            .rows;
            Ok((
                spans,
                completions,
                profiler,
                list,
                recorded_source(index, execution_id),
            ))
        })?
        else {
            return Err(TelemetryError::failed("no recordings in this project"));
        };
        let (spans, completions, profiler, list, recorded) = opened;
        let completions: HashMap<&str, &Vec<Value>> = completions
            .iter()
            .filter_map(|row| Some((row[0].as_str()?, row)))
            .collect();
        let started_ns = |row: &[Value]| row[6].as_str().and_then(unix_ns);
        let root_start = spans.first().and_then(|root| started_ns(root));
        let offset = |ns: Option<i64>| ns.zip(root_start).map(|(ns, start)| ns - start);

        let mut threads = Vec::new();
        let mut calls = Vec::new();
        let mut errors = Vec::new();
        for span in &spans {
            let id = span[0].as_str().unwrap_or_default();
            let done = completions.get(id).copied();
            let started = offset(started_ns(span));
            let ended = offset(done.and_then(|d| d[2].as_str()).and_then(unix_ns));
            if span[1] == "future" {
                threads.push(thread(span, done, started, ended, id == execution_id));
            } else {
                let call = call(span, done, started, ended);
                if let Some(error) = done.and_then(|done| error_capture(&call, done)) {
                    errors.push(error);
                }
                calls.push(call);
            }
        }
        let calls_retained = calls.len();
        let threads_total = threads.len();
        let execution_row = list.first().map(|row| {
            let mut execution = execution(row);
            execution["sourceState"] =
                Value::from(source_state(recorded.as_deref(), current_source));
            execution["callsRetained"] = Value::from(calls_retained);
            execution["threadsTotal"] = Value::from(threads_total);
            execution
        });
        Ok(json!({
            "execution": execution_row,
            "threads": threads,
            "callPaths": call_paths(&profiler),
            "calls": calls,
            "errors": errors,
        }))
    }
}

/// `LIST_SQL` for one root call.
const LIST_SQL_ONE: &str = "
SELECT a.span_id, a.span_name, s.status, a.start_time, s.duration, p.status
FROM span_announcements a
LEFT JOIN spans s ON s.span_id = a.span_id
LEFT JOIN processes p ON p.process_id = a.process_id";

/// The source fingerprint of the recording that holds `span_id`, which the
/// tables do not carry: the index's own record of it.
fn recorded_source(index: &Index, span_id: &str) -> Option<String> {
    let (recording, _) = span_id.split_once(':')?;
    index
        .connection()
        .query_row(
            "SELECT lower(hex(source_snapshot_id)) FROM recording
             WHERE lower(hex(recording_id)) = ?1",
            [recording],
            |r| r.get(0),
        )
        .ok()
        .flatten()
}

/// Unix nanoseconds of an RFC 3339 UTC timestamp as the tables write them.
fn unix_ns(text: &str) -> Option<i64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let nanos: i64 = format!("{fraction:0<9}").get(..9)?.parse().ok()?;
    // Howard Hinnant's days-from-civil (proleptic Gregorian).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + hour * 3600 + minute * 60 + second) * 1_000_000_000) + nanos)
}

fn execution(row: &[Value]) -> Value {
    let started_ms = row[3].as_str().and_then(unix_ns).map(|ns| ns / 1_000_000);
    let status = match (row[2].as_str(), row[5].as_str()) {
        (Some("return"), _) => "succeeded",
        (Some("user_error"), _) => "failed",
        (Some("panic_error"), _) => "panicked",
        (Some("cancel_error"), _) => "cancelled",
        // No completion yet: its process says whether it can still end.
        (None, Some("running")) => "running",
        _ => "incomplete",
    };
    json!({
        "executionId": row[0],
        "entryFqn": row[1],
        "sourceLabel": null,
        "revisionId": null,
        "status": status,
        "indexState": if row[2].is_null() { "no_root_ended" } else { "complete" },
        "valueState": null,
        "startedAtMs": started_ms,
        "durationNs": row[4],
        // Every invocation is counted in the process's profiler, not per call.
        "totalCalls": null,
        "totalErrors": null,
        "callsRetained": null,
        "threadsTotal": null,
    })
}

/// Whether recorded locations can be trusted against today's files.
fn source_state(recorded: Option<&str>, current: Option<[u8; 32]>) -> &'static str {
    match (recorded, current) {
        (Some(recorded), Some(current)) if recorded == hex(&current) => "verified",
        (Some(_), Some(_)) => "stale",
        _ => "unverified",
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn thread(
    span: &[Value],
    done: Option<&Vec<Value>>,
    started: Option<i64>,
    ended: Option<i64>,
    root: bool,
) -> Value {
    json!({
        "threadId": span[0],
        "parentThreadId": span[4],
        "spawnCallId": if root { Value::Null } else { span[3].clone() },
        "spawnFqn": if root { Value::Null } else { span[2].clone() },
        "spawnSiteFile": null,
        "spawnSiteLine": null,
        "spawnSiteState": null,
        "name": if root { Value::Null } else { span[2].clone() },
        "kind": if root { "root" } else { "spawn" },
        "startedNs": started,
        "endedNs": ended,
        "endStatus": match done.and_then(|d| d[1].as_str()) {
            Some("return") => Value::from("completed"),
            Some("user_error" | "panic_error") => Value::from("errored"),
            Some("cancel_error") => Value::from("cancelled"),
            _ => Value::Null,
        },
    })
}

/// A captured value's UI state from `baml_value_state` and its rendering.
fn captured(state: Option<&str>, value: &Value) -> (&'static str, Value) {
    match (state, value) {
        (Some("present" | "null" | "omitted"), value) => {
            ("available", Value::String(value.to_string()))
        }
        (Some("capture_pending"), _) => ("lost:pending", Value::Null),
        (Some("no_value") | None, _) => ("not_captured", Value::Null),
        _ => ("lost:unavailable", Value::Null),
    }
}

fn call(
    span: &[Value],
    done: Option<&Vec<Value>>,
    started: Option<i64>,
    ended: Option<i64>,
) -> Value {
    let status = match done.and_then(|d| d[1].as_str()) {
        Some("return") => "ok",
        Some("user_error" | "panic_error") => "errored",
        Some("cancel_error") => "cancelled",
        _ => "incomplete",
    };
    let not_applicable = || ("not_applicable", Value::Null);
    let (args_state, args) = match done {
        Some(d) => captured(d[7].as_str(), &d[4]),
        None => ("lost:pending", Value::Null),
    };
    let (output_state, output) = match done {
        Some(d) if status == "ok" => captured(d[8].as_str(), &d[5]),
        _ => not_applicable(),
    };
    let (error_state, error) = match done {
        Some(d) if status == "errored" || status == "cancelled" => captured(d[9].as_str(), &d[6]),
        _ => not_applicable(),
    };
    // A function span's parent is a call, or the future it runs on.
    let parent = (span[3] != span[4]).then(|| span[3].clone());
    json!({
        "callId": span[0],
        "parentCallId": parent,
        "threadId": span[4],
        "callPathId": span[5],
        "fqn": span[2],
        "kind": null,
        "edgeKind": null,
        "callSiteFile": null,
        "callSiteLine": null,
        "callSiteState": null,
        "errorOccurrenceId": null,
        "errorLinked": false,
        "startedNs": started,
        "endedNs": ended,
        "durationNs": done.map_or(Value::Null, |d| d[3].clone()),
        "status": status,
        "selectionReasons": [],
        "argsState": args_state,
        "outputState": output_state,
        "errorState": error_state,
        "argsCid": null,
        "outputCid": null,
        "errorCid": null,
        "errorId": if status == "errored" { span[0].clone() } else { Value::Null },
        "args": args,
        "output": output,
        "error": error,
    })
}

/// A failed call's error, with the stack its `baml.errors.Context` carries.
fn error_capture(call: &Value, done: &[Value]) -> Option<Value> {
    if call["status"] != "errored" {
        return None;
    }
    let frames = done[10].as_array().cloned().unwrap_or_default();
    let stack: Vec<Value> = frames
        .iter()
        .map(|frame| frame["function_name"].clone())
        .collect();
    let thrown = frames.last();
    Some(json!({
        "errorId": call["callId"],
        "grain": "errored_call",
        "callId": call["callId"],
        "callFqn": call["fqn"],
        "callThreadId": call["threadId"],
        "throwCallId": null,
        "throwThreadId": null,
        "throwCallPathId": null,
        "throwFqn": thrown.map_or(Value::Null, |f| f["function_name"].clone()),
        "throwSiteFile": thrown.map_or(Value::Null, |f| f["file"].clone()),
        "throwSiteLine": thrown.map_or(Value::Null, |f| f["line"].clone()),
        "kind": if done[1] == "panic_error" { "panic" } else { "throw" },
        "source": null,
        "valueState": call["errorState"],
        "valueCid": null,
        "stackComplete": !stack.is_empty(),
        "stack": stack,
        "value": call["error"],
    }))
}

/// The process's profiler, in the calling-context shape the UI reads.
fn call_paths(rows: &[Vec<Value>]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let (total, own) = (row[7].as_i64(), row[8].as_i64());
            json!({
                "callPathId": row[0],
                "parentCallPathId": row[1],
                "depth": null,
                "fqn": row[2],
                "kind": null,
                "origin": null,
                "edgeKind": if row[1].is_null() { "root" } else { "call" },
                "callSiteFile": null,
                "callSiteLine": null,
                "callSiteStart": null,
                "callSiteEnd": null,
                "callSiteState": null,
                "callsStarted": null,
                "callsSelected": null,
                "completedCalls": row[3],
                "completedOk": row[4],
                "completedError": row[5].as_i64().zip(row[6].as_i64()).map(|(e, c)| e - c),
                "completedCancelled": row[6],
                "outcomeState": "recorded",
                "inclusiveNs": row[7],
                "directChildNs": total.zip(own).map(|(t, o)| t - o),
                "awaitNs": null,
                "selfNs": row[8],
                "ioNs": row[9],
                "timingComplete": own.is_some(),
                "overflowReason": null,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
