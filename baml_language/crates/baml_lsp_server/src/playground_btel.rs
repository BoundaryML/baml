//! Playground telemetry reads over the project's local Btel recordings.
//!
//! Every read goes through `baml_query_btel`, exactly as `baml query` does:
//! an explicit refresh applies newly completed recording files, then the SQL
//! below runs in a fresh read snapshot with a fresh value cache. The open
//! index is reused across requests. All of it is blocking work: callers run
//! it on a blocking thread, never on the async executor.
//!
//! An execution opens with its threads (timeline lanes), its calling
//! contexts with completed-call counts and recursion-aware timing, its
//! retained calls with their captured values, and its errors. Call, spawn
//! and throw sites come from the recording's own source maps; whether the
//! file still matches is checked against the program the playground has
//! currently built. What recordings do not carry arrives empty or null
//! rather than invented: invocation starts, thread names and media bytes.
//! Recordings without error evidence fall back to errored calls.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, TryLockError},
};

use baml_query_btel::{Index, IndexOptions, QueryRequest, QueryResult, SqlParam};
use serde_json::{Value, json};

/// Rows the list shows; the newest executions first.
const LIST_SQL: &str = "
SELECT e.execution_id, e.entry_fqn, e.status, e.started_at_ms, e.duration_ns,
  e.calls_retained, e.threads_total, e.completed_calls, r.prefix_state
FROM executions e
JOIN recordings r ON r.recording_id = e.recording_id
ORDER BY e.started_at_ms DESC
LIMIT 200";

const EXECUTION_SQL: &str = "
SELECT e.execution_id, e.entry_fqn, e.status, e.started_at_ms, e.duration_ns,
  e.calls_retained, e.threads_total, e.completed_calls, r.prefix_state,
  r.source_snapshot_id, r.format_minor
FROM executions e
JOIN recordings r ON r.recording_id = e.recording_id
WHERE e.execution_id = ?1";

/// Timeline lanes: the execution's threads, offsets from its start.
const THREADS_SQL: &str = "
SELECT thread_id, parent_thread_id, spawn_call_id, spawn_fqn, kind, start_offset_ns,
  end_offset_ns, end_status, spawn_site_file, spawn_site_line, spawn_site_state
FROM threads WHERE execution_id = ?1
ORDER BY start_offset_ns";

/// Calling contexts with completed-call counts (never invocation starts).
const CALL_PATHS_SQL: &str = "
SELECT s.call_path_id, s.parent_call_path_id, s.depth, s.fqn, s.kind, s.origin, s.edge_kind,
  s.completed_calls, s.inclusive_ns, s.direct_child_ns, s.await_ns, s.self_ns,
  s.self_time_state, s.ok_calls, s.errored_calls, s.cancelled_calls, s.outcome_state,
  p.call_site_file, p.call_site_line, p.call_site_start, p.call_site_end, p.call_site_state
FROM call_path_stats s
LEFT JOIN call_paths p ON p.call_path_id = s.call_path_id
WHERE s.execution_id = ?1
ORDER BY s.depth, s.call_path_id";

/// Retained calls with their captured values, rendered server-side.
const CALLS_SQL: &str = "
SELECT c.call_id, c.parent_call_id, c.thread_id, c.fqn, c.status, c.start_offset_ns,
  c.duration_ns, c.args_state, c.output_state, c.error_state, c.args_cid, c.output_cid,
  c.error_cid, c.args, c.output, c.error, c.call_path_id, c.kind, p.edge_kind,
  c.call_site_file, c.call_site_line, c.call_site_state, c.error_occurrence_id,
  c.error_link_state
FROM calls c
LEFT JOIN call_paths p ON p.call_path_id = c.call_path_id
WHERE c.execution_id = ?1
ORDER BY c.start_offset_ns";

/// Every raise of the execution, in raise order. Values stay unread: an
/// occurrence's value comes from its failed calls, which `CALLS_SQL` renders.
const RAISES_SQL: &str = "
SELECT raise_id, thread_id, kind, occurrence_id, origin_state, origin_via, origin_candidates,
  unresolved_reason, fqn, call_id, site_state, site_file, site_line, site_start, site_end,
  unwind_result, handler_fqn, failed_calls, stack_depth, stack_state, inherited_trace
FROM error_raises WHERE execution_id = ?1
ORDER BY raised_ticks, raise_id";

/// Stacks of the raises that start or cannot join an occurrence.
const FRAMES_SQL: &str = "
SELECT f.raise_id, f.position, f.fqn, f.native, f.site_file, f.site_line
FROM error_frames f
WHERE f.raise_id IN (SELECT raise_id FROM error_raises
  WHERE execution_id = ?1 AND origin_state != 'proven')
ORDER BY f.raise_id, f.position";

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
        let mut indexes = self.indexes.lock().expect("index table");
        if let Some(index) = indexes.get(project) {
            return Ok(Some(Arc::clone(index)));
        }
        let index = Index::for_project(project, IndexOptions::default())
            .map_err(|e| TelemetryError::failed(e.to_string()))?;
        // Nothing recorded yet: do not cache an empty in-memory index, so the
        // first recording is found by the next request.
        if index.source_missing() {
            return Ok(None);
        }
        let index = Arc::new(Mutex::new(index));
        indexes.insert(project.to_owned(), Arc::clone(&index));
        Ok(Some(index))
    }

    /// Reconcile new files, then run each query in its own read snapshot.
    fn read(
        &self,
        project: &str,
        queries: &[(&str, Vec<SqlParam>)],
    ) -> Result<Option<Vec<QueryResult>>, TelemetryError> {
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
        queries
            .iter()
            .map(|(sql, params)| {
                index
                    .query(&QueryRequest {
                        sql: (*sql).to_owned(),
                        params: params.clone(),
                        ..QueryRequest::default()
                    })
                    .map_err(|e| TelemetryError::failed(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// Execution list and whether the project has no recordings yet.
    pub fn list_executions(&self, project: &str) -> Result<(Vec<Value>, bool), TelemetryError> {
        match self.read(project, &[(LIST_SQL, Vec::new())])? {
            None => Ok((Vec::new(), true)),
            Some(results) => Ok((
                results[0].rows.iter().map(|row| execution(row)).collect(),
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
        let id = SqlParam::Text(execution_id.to_owned());
        let Some(results) = self.read(
            project,
            &[
                (EXECUTION_SQL, vec![id.clone()]),
                (THREADS_SQL, vec![id.clone()]),
                (CALL_PATHS_SQL, vec![id.clone()]),
                (CALLS_SQL, vec![id.clone()]),
                (RAISES_SQL, vec![id.clone()]),
                (FRAMES_SQL, vec![id]),
            ],
        )?
        else {
            return Err(TelemetryError::failed("no recordings in this project"));
        };
        let source_state = results[0]
            .rows
            .first()
            .map_or("unverified", |row| source_state(&row[9], current_source));
        let execution_row = results[0].rows.first().map(|row| {
            let mut execution = execution(row);
            execution["sourceState"] = Value::from(source_state);
            execution
        });
        let has_error_evidence = results[0]
            .rows
            .first()
            .and_then(|row| row[10].as_i64())
            .is_some_and(|minor| minor >= 2);
        let threads: Vec<Value> = results[1].rows.iter().map(|row| thread(row)).collect();
        let call_paths: Vec<Value> = results[2].rows.iter().map(|row| call_path(row)).collect();
        let calls: Vec<Value> = results[3].rows.iter().map(|row| call(row)).collect();
        let mut errors = if has_error_evidence {
            errors::occurrences(&results[4].rows, &results[5].rows, &calls)
        } else {
            Vec::new()
        };
        // Failed calls no raise accounts for: every one in an older recording,
        // and calls the engine ended without unwinding in a newer one.
        errors.extend(
            calls
                .iter()
                .filter(|call| call["status"] == "errored" && call["errorLinked"] != true)
                .map(error_capture),
        );
        Ok(json!({
            "execution": execution_row,
            "threads": threads,
            "callPaths": call_paths,
            "calls": calls,
            "errors": errors,
        }))
    }
}

fn execution(row: &[Value]) -> Value {
    let status = row[2].as_str().unwrap_or("incomplete");
    let prefix_state = row[8].as_str().unwrap_or("");
    json!({
        "executionId": row[0],
        "entryFqn": row[1],
        "sourceLabel": null,
        "revisionId": null,
        "status": match status {
            "ok" => "succeeded",
            "errored" => "failed",
            "cancelled" => "cancelled",
            // No completion indexed (or conflicting ones): not proof that it
            // is still running, nor that it was abandoned.
            _ => "incomplete",
        },
        "indexState": if status == "incomplete" || status == "conflicted" {
            "no_root_ended"
        } else if prefix_state == "complete_prefix" {
            "complete"
        } else {
            "partial"
        },
        "valueState": null,
        "startedAtMs": row[3],
        "durationNs": row[4],
        // Completed invocations of every call, retained or not.
        "totalCalls": row[7],
        // This lightweight list query has no population-outcome rollup.
        // The detail view can sum its full set of calling contexts.
        "totalErrors": null,
        "callsRetained": row[5],
        "threadsTotal": row[6],
    })
}

/// Whether recorded locations can be trusted against today's files.
fn source_state(recorded: &Value, current: Option<[u8; 32]>) -> &'static str {
    let recorded = recorded.as_str();
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

fn thread(row: &[Value]) -> Value {
    json!({
        "threadId": row[0],
        "parentThreadId": row[1],
        "spawnCallId": row[2],
        "spawnFqn": row[3],
        "spawnSiteFile": row[8],
        "spawnSiteLine": row[9],
        "spawnSiteState": row[10],
        // Thread names are not recorded.
        "name": null,
        "kind": row[4],
        "startedNs": row[5],
        "endedNs": row[6],
        "endStatus": match row[7].as_str() {
            Some("ok") => Value::from("completed"),
            Some(status @ ("errored" | "cancelled")) => Value::from(status),
            _ => Value::Null,
        },
    })
}

fn call_path(row: &[Value]) -> Value {
    let self_state = row[12].as_str().unwrap_or("");
    json!({
        "callPathId": row[0],
        "parentCallPathId": row[1],
        "depth": row[2],
        "fqn": row[3],
        "kind": row[4],
        "origin": row[5],
        "edgeKind": row[6],
        "callSiteFile": row[17],
        "callSiteLine": row[18],
        "callSiteStart": row[19],
        "callSiteEnd": row[20],
        "callSiteState": row[21],
        // Recordings count completed invocations, not starts or selections.
        "callsStarted": null,
        "callsSelected": null,
        "completedCalls": row[7],
        "completedOk": row[13],
        "completedError": row[14],
        "completedCancelled": row[15],
        "outcomeState": row[16],
        "inclusiveNs": row[8],
        "directChildNs": row[9],
        "awaitNs": row[10],
        "selfNs": row[11],
        "timingComplete": self_state == "valid",
        "overflowReason": null,
    })
}

/// A captured value's UI state from its evidence state and rendered text.
fn captured(state: &str, value: &Value) -> (&'static str, Value) {
    match (state, value) {
        ("not_applicable", _) => ("not_applicable", Value::Null),
        // A recorded capture the reader could not produce.
        ("reference", Value::Null) => ("lost:unavailable", Value::Null),
        ("reference", value) => ("available", Value::String(value.to_string())),
        ("pending", _) => ("lost:pending", Value::Null),
        ("conflicted", _) => ("lost:conflicted", Value::Null),
        _ => ("not_captured", Value::Null),
    }
}

fn call(row: &[Value]) -> Value {
    let status = row[4].as_str().unwrap_or("incomplete");
    let (started, duration) = (row[5].as_i64(), row[6].as_i64());
    let (args_state, args) = captured(row[7].as_str().unwrap_or(""), &row[13]);
    let (output_state, output) = captured(row[8].as_str().unwrap_or(""), &row[14]);
    let (error_state, error) = captured(row[9].as_str().unwrap_or(""), &row[15]);
    json!({
        "callId": row[0],
        "parentCallId": row[1],
        "threadId": row[2],
        "callPathId": row[16],
        "fqn": row[3],
        "kind": row[17],
        "edgeKind": row[18],
        // Null for a direct recursive re-entry: its own site is not recorded.
        "callSiteFile": row[19],
        "callSiteLine": row[20],
        "callSiteState": row[21],
        "errorOccurrenceId": row[22],
        "errorLinked": row[23] == "linked",
        "startedNs": started,
        "endedNs": started.zip(duration).map(|(s, d)| s + d),
        "durationNs": duration,
        // `incomplete`: no completion indexed. Older backends sent null for a
        // call that never returned, which the UI reads as running.
        "status": match status {
            "ok" | "errored" | "cancelled" => status,
            _ => "incomplete",
        },
        "selectionReasons": [],
        "argsState": args_state,
        "outputState": output_state,
        "errorState": error_state,
        "argsCid": row[10],
        "outputCid": row[11],
        "errorCid": row[12],
        "errorId": if status == "errored" { row[0].clone() } else { Value::Null },
        "args": args,
        "output": output,
        "error": error,
    })
}

/// An error value observed on a retained call that ended in an error, with
/// no recorded raise: which call threw it, where, and whether it was
/// rethrown are unknown, so the throw fields stay null and `grain` tells the
/// UI this is an errored call, not a distinct error.
fn error_capture(call: &Value) -> Value {
    json!({
        "errorId": call["callId"],
        "grain": "errored_call",
        "callId": call["callId"],
        "callFqn": call["fqn"],
        "callThreadId": call["threadId"],
        "throwCallId": null,
        "throwThreadId": null,
        "throwCallPathId": null,
        "throwFqn": null,
        "throwSiteFile": null,
        "throwSiteLine": null,
        "kind": null,
        "source": null,
        "valueState": call["errorState"],
        "valueCid": call["errorCid"],
        "stackComplete": false,
        "stack": [],
        "value": call["error"],
    })
}

mod errors;

#[cfg(test)]
mod tests;
