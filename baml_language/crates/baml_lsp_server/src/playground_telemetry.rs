//! Telemetry reads over the canonical `profiles-v1` store.
//!
//! Structure and timing left the run store when the old profiler was retired
//! (`bex_events::run::Run` carries payloads and diagnostics only, and
//! `run_wire` asserts the wire has no `calls`/`threads`). Both now live in
//! `.baml/profiles-v1`, so the playground reads them here.
//!
//! Every read goes through a `QuerySession` bound to one project's store
//! root, exactly as `baml query` does. `baml_query_profiles` deliberately
//! keeps its store types crate-private so this consumer cannot walk the
//! store around the session's snapshot, budget, and authorization seam; the
//! SQL below is authored here and never accepted from the client.
//!
//! Binding freezes a snapshot of the committed prefix, so each request binds
//! a fresh session: a list taken now must be able to see a run that finished
//! a second ago.

use std::path::{Path, PathBuf};

use baml_query::{
    budget::{CancellationToken, QueryBudgets},
    catalog::CatalogProfile,
    value::{
        model::{MediaContent, Value},
        resolver::{DecodeCaps, Resolved, ValueResolver as _},
    },
};
use base64::Engine as _;
use bex_prof_store::prof::backend::ProfilerSession;
use datafusion::arrow::{
    array::{Array, BooleanArray, StringArray, TimestampNanosecondArray, UInt32Array, UInt64Array},
    record_batch::RecordBatch,
};
use serde::Serialize;

/// Rows returned per relation. The playground opens one execution at a time,
/// so these bound a single execution's evidence, not the whole store.
const MAX_ROWS: u64 = 200_000;

/// Ceiling for one hydrated value. Media renders as a descriptor rather than
/// its bytes, so this bounds structural payloads, not images.
const MAX_VALUE_BYTES: u64 = 4 << 20;

/// Ceiling across all hydrations in one request.
const MAX_DECODED_BYTES: u64 = 64 << 20;

/// Executions, newest first. Root threads only: a thread with no parent *is*
/// an execution, and the execution-level columns are non-NULL only there.
///
/// Ordering is by wall clock, not by `started_ns`. That column is relative
/// to its own process's start, so comparing it across processes ranks a run
/// that began late inside a long-lived process above one that began minutes
/// later in a fresh one -- which is exactly backwards for a list whose whole
/// claim is "newest first". Rows without a clock anchor sort last rather
/// than jumping to the top.
const EXECUTIONS_SQL: &str = "
    SELECT execution_id, entry_fqn, source_label, revision_id, status,
           index_state, value_state, started_at, duration_ns, total_calls,
           total_errors, calls_retained, threads_total
    FROM threads_v1
    WHERE parent_thread_id IS NULL
    ORDER BY started_at DESC NULLS LAST
    LIMIT 200
";

/// One execution by id. The list query is capped, so filtering its rows in
/// Rust would leave an older execution with threads and calls populated but
/// no execution row at all.
const EXECUTION_BY_ID_SQL: &str = "
    SELECT execution_id, entry_fqn, source_label, revision_id, status,
           index_state, value_state, started_at, duration_ns, total_calls,
           total_errors, calls_retained, threads_total
    FROM threads_v1
    WHERE parent_thread_id IS NULL AND execution_id = $1
";

/// One execution's thread lanes, including the root.
const THREADS_SQL: &str = "
    SELECT thread_id, parent_thread_id, spawn_call_id, spawn_fqn,
           spawn_site_file, spawn_site_line, name, kind, started_ns,
           ended_ns, end_status
    FROM threads_v1
    WHERE execution_id = $1
    ORDER BY started_ns
";

/// The calling-context tree: population-true aggregates, one row per path.
const CALL_PATHS_SQL: &str = "
    SELECT call_path_id, parent_call_path_id, depth, fqn, kind, origin,
           edge_kind, call_site_file, call_site_line, call_site_start,
           call_site_end, calls_started, calls_selected, completed_ok,
           completed_error, completed_cancelled, inclusive_ns,
           direct_child_ns, await_ns, self_ns, timing_complete,
           overflow_reason
    FROM call_path_stats_v1
    WHERE execution_id = $1
";

/// Individually retained spans. Bounded by capture policy: this is never all
/// calls, and `call_path_stats_v1.calls_started` is what covers those.
/// Individually retained spans, with their captured values.
///
/// `args`, `output`, and `error` are virtual columns: selecting them
/// hydrates through the resolver. Media does not travel here -- a decoded
/// image renders as `{"$media":…,"bytes_len":N}`, so a megabyte PNG costs a
/// few dozen bytes on this path and the bytes are fetched per value, on
/// demand, only when someone looks at them.
const CALLS_SQL: &str = "
    SELECT call_id, parent_call_id, thread_id, call_path_id, fqn, kind,
           edge_kind, call_site_file, call_site_line, started_ns, ended_ns,
           duration_ns, status, selection_reasons, args_state, output_state,
           error_state, args_cid, output_cid, error_cid, error_id,
           args, output, error
    FROM calls_v1
    WHERE execution_id = $1
    ORDER BY started_ns
";

/// Captured errors, with the root-to-throw stack the capture recorded.
///
/// `value` is a virtual column: selecting it hydrates the captured error
/// through the resolver, which is what puts the provider's actual message
/// on screen instead of "an error occurred". Errors are few and bounded per
/// execution, so the hydration cost is worth paying here; argument and
/// output values are not selected for the same reason in reverse.
const ERRORS_SQL: &str = "
    SELECT error_id, throw_call_id, throw_thread_id, throw_call_path_id,
           throw_fqn, throw_site_file, throw_site_line, kind, source,
           value_state, value_cid, stack_complete, stack, value
    FROM errors_v1
    WHERE execution_id = $1
";

/// Resolve the store a project's profiles are written to. Producer and
/// reader share this precedence (`BAML_PROFILE_DIR` wins), so the two sides
/// cannot disagree about where the data is.
#[must_use]
pub fn store_root_for_project(project_root: &Path) -> PathBuf {
    ProfilerSession::resolve_store_root(project_root)
}

/// Point the global profiler session's store at this project, mirroring what
/// the CLI does at project load. Without it the session falls back to a
/// *relative* `.baml/profiles-v1`, which for a long-lived server resolves
/// against whatever directory it happened to start in.
///
/// The profiler's store root is process-global and set once, so a server
/// holding several workspace roots writes every run it starts into the
/// first root's store, while [`store_root_for_project`] reads each project
/// from its own. Runs a project starts elsewhere -- from the CLI, say --
/// still land in that project's store and read back correctly; it is only
/// playground-initiated runs in a second root that are recorded under the
/// first. Making that per-project needs a profiler session per project,
/// which the engine's single global session does not offer, so this reports
/// the limit instead of hiding it.
pub fn configure_store_root(project_root: &Path) {
    let store = project_root.join(".baml/profiles-v1");
    if ProfilerSession::configure_global_store_root(store.clone()) {
        tracing::info!("Telemetry: profiler writes to {}", store.display());
    } else {
        tracing::debug!(
            "Telemetry: profiler store root already set; {} not applied",
            store.display()
        );
    }
}

/// Warn when the workspace has more than one root, because runs this server
/// starts in the others are recorded under the first.
pub fn warn_if_multi_root(workspace_roots: &[std::path::PathBuf]) {
    if workspace_roots.len() <= 1 {
        return;
    }
    let others: Vec<String> = workspace_roots[1..]
        .iter()
        .map(|root| root.display().to_string())
        .collect();
    tracing::warn!(
        "Telemetry: the profiler writes one store per process, so runs started \
         here for {} are recorded under {}. Those projects will look empty in \
         the Telemetry tab; run them from the CLI to record them in place.",
        others.join(", "),
        workspace_roots[0].display()
    );
}

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// No store yet: nothing has run under this project since the profiler
    /// started writing. This is an empty state, not a failure.
    #[error("no profile store at {0}")]
    NoStore(PathBuf),
    #[error("{0}")]
    Query(String),
}

/// One execution: a row in the executions table.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRow {
    pub execution_id: String,
    pub entry_fqn: Option<String>,
    pub source_label: Option<String>,
    pub revision_id: Option<String>,
    pub status: Option<String>,
    /// `complete` | `no_root_ended` | `root_started_lost` | `index_corrupt`.
    /// Anything but `complete` means the row below it is partial evidence.
    pub index_state: Option<String>,
    /// `complete` | `partial` | `none` — whether captured values survived.
    pub value_state: Option<String>,
    pub started_at_ms: Option<i64>,
    pub duration_ns: Option<u64>,
    /// Population total: every call, retained or not.
    pub total_calls: Option<u64>,
    pub total_errors: Option<u64>,
    /// Spans with a retained start. Always `<= total_calls`; the difference
    /// is what the UI shows as summarized-only work.
    pub calls_retained: Option<u64>,
    pub threads_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRow {
    pub thread_id: String,
    pub parent_thread_id: Option<String>,
    pub spawn_call_id: Option<String>,
    pub spawn_fqn: Option<String>,
    pub spawn_site_file: Option<String>,
    pub spawn_site_line: Option<u32>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub started_ns: Option<u64>,
    pub ended_ns: Option<u64>,
    pub end_status: Option<String>,
}

/// One calling-context aggregate. Counts cover every call that took this
/// path; there is no per-instance ordering or timestamp here by construction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallPathRow {
    pub call_path_id: String,
    pub parent_call_path_id: Option<String>,
    pub depth: Option<u32>,
    pub fqn: Option<String>,
    pub kind: Option<String>,
    pub origin: Option<String>,
    /// `root` | `call` | `spawn` — spawn means this path runs on its own
    /// logical thread, so its wall time overlaps its parent's.
    pub edge_kind: Option<String>,
    pub call_site_file: Option<String>,
    pub call_site_line: Option<u32>,
    pub call_site_start: Option<u32>,
    pub call_site_end: Option<u32>,
    pub calls_started: Option<u64>,
    pub calls_selected: Option<u64>,
    pub completed_ok: Option<u64>,
    pub completed_error: Option<u64>,
    pub completed_cancelled: Option<u64>,
    pub inclusive_ns: Option<u64>,
    pub direct_child_ns: Option<u64>,
    pub await_ns: Option<u64>,
    /// `inclusive_ns - direct_child_ns - await_ns`: the three are disjoint
    /// components of inclusive time, so summing `self_ns` across paths is a
    /// valid CPU total and summing `await_ns` a valid waiting total.
    pub self_ns: Option<u64>,
    /// False when a counter saturated or self time underflowed. The UI must
    /// not present a derived percentage from a row that says this.
    pub timing_complete: Option<bool>,
    /// Non-NULL only on synthetic overflow rows, which stand in for paths
    /// the tree could not afford to keep separately.
    pub overflow_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRow {
    pub call_id: String,
    pub parent_call_id: Option<String>,
    pub thread_id: Option<String>,
    /// The join back to the aggregate. Exact, not inferred from the name.
    pub call_path_id: Option<String>,
    pub fqn: Option<String>,
    pub kind: Option<String>,
    pub edge_kind: Option<String>,
    pub call_site_file: Option<String>,
    pub call_site_line: Option<u32>,
    pub started_ns: Option<u64>,
    pub ended_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub status: Option<String>,
    /// Why this call was kept: `root` | `llm` | `manual`.
    pub selection_reasons: Vec<String>,
    /// `available` | `not_captured` | `lost:<reason>` | `not_applicable`.
    /// `not_captured` and `lost:` are different facts and the UI says so.
    pub args_state: Option<String>,
    pub output_state: Option<String>,
    pub error_state: Option<String>,
    pub args_cid: Option<String>,
    pub output_cid: Option<String>,
    pub error_cid: Option<String>,
    pub error_id: Option<String>,
    /// Hydrated captured values, rendered. None when nothing was captured
    /// or the capture was lost; the matching `*_state` says which.
    pub args: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRow {
    pub error_id: String,
    pub throw_call_id: Option<String>,
    pub throw_thread_id: Option<String>,
    pub throw_call_path_id: Option<String>,
    pub throw_fqn: Option<String>,
    pub throw_site_file: Option<String>,
    pub throw_site_line: Option<u32>,
    /// `fresh` | `rethrow`.
    pub kind: Option<String>,
    pub source: Option<String>,
    pub value_state: Option<String>,
    pub value_cid: Option<String>,
    /// False when the stack has gaps, so the UI must not present it as the
    /// complete path from root to throw.
    pub stack_complete: Option<bool>,
    pub stack: Vec<String>,
    /// The captured error, hydrated. None when the capture was lost or no
    /// policy selected one; `value_state` distinguishes those.
    pub value: Option<String>,
}

/// One execution's complete evidence, in the four grains the catalog serves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionTelemetry {
    pub execution: Option<ExecutionRow>,
    pub threads: Vec<ThreadRow>,
    pub call_paths: Vec<CallPathRow>,
    pub calls: Vec<CallRow>,
    pub errors: Vec<ErrorRow>,
}

/// The largest media payload served to the client in one read. A browser
/// gets this as a data URL, so it is held in memory whole on both sides.
const MAX_MEDIA_BYTES: u64 = 24 << 20;

/// One media payload, ready for an `<img src>`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaBody {
    /// `image`, `audio`, `pdf`.
    pub kind: String,
    pub mime: String,
    /// Base64 of the bytes, or None when the value carried a URL instead.
    pub base64: Option<String>,
    /// Set when the media was a reference rather than stored bytes.
    pub url: Option<String>,
    pub bytes_len: Option<u64>,
}

/// Read one captured value's media by content id.
///
/// Values reach the client as structure with media rendered as a descriptor,
/// which keeps a megabyte image off the panel's critical path. This is the
/// second step: the bytes for one value, fetched when someone actually looks
/// at it. Resolution goes through `ProfilesResolver`, the CAS-reading half of
/// the query stack that `baml_query_profiles` exports for exactly this.
pub fn read_media(project_root: &Path, cid_hex: &str) -> Result<MediaBody, TelemetryError> {
    let store_root = store_root_for_project(project_root);
    if !store_root.exists() {
        return Err(TelemetryError::NoStore(store_root));
    }
    let cid = parse_cid(cid_hex)?;
    let resolver = baml_query_profiles::ProfilesResolver::new(store_root);
    let caps = DecodeCaps {
        max_bytes: MAX_MEDIA_BYTES,
        max_depth: 64,
    };
    match resolver.resolve_cid(&cid, caps) {
        Resolved::Value(value) => first_media(&value)
            .ok_or_else(|| TelemetryError::Query("this value holds no media".to_string())),
        Resolved::Unavailable(reason) => Err(TelemetryError::Query(format!(
            "value unavailable: {reason:?}"
        ))),
    }
}

/// Values are handed to the UI by the id of the capture they came from, and
/// a capture is one value: the first media found in it is that value's
/// media. Walking is what makes `Fn(art: image)` work, where the image is a
/// field of the argument object rather than the whole of it.
fn first_media(value: &Value) -> Option<MediaBody> {
    match value {
        Value::Media {
            kind,
            mime,
            content,
        } => Some(match content {
            MediaContent::Bytes(bytes) => MediaBody {
                base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes.as_slice())),
                bytes_len: Some(bytes.len() as u64),
                kind: kind.clone(),
                mime: mime.clone(),
                url: None,
            },
            MediaContent::Url(url) => MediaBody {
                base64: None,
                bytes_len: None,
                kind: kind.clone(),
                mime: mime.clone(),
                url: Some(url.clone()),
            },
        }),
        Value::List(items) => items.iter().find_map(first_media),
        Value::Class { fields, .. } => fields
            .iter()
            .filter_map(|(_, _, field)| field.as_ref())
            .find_map(first_media),
        Value::Map(entries) => entries.iter().find_map(|(_, entry)| first_media(entry)),
        _ => None,
    }
}

/// Value cids reach the client as `bamlv_1_<64 hex>`; only the hex is the
/// content id.
fn parse_cid(cid: &str) -> Result<[u8; 32], TelemetryError> {
    let hex_part = cid.strip_prefix("bamlv_1_").unwrap_or(cid);
    let bytes = hex::decode(hex_part)
        .map_err(|_| TelemetryError::Query(format!("not a value id: {cid}")))?;
    bytes
        .try_into()
        .map_err(|_| TelemetryError::Query(format!("not a value id: {cid}")))
}

/// List executions in a project's store, newest first.
pub async fn list_executions(project_root: &Path) -> Result<Vec<ExecutionRow>, TelemetryError> {
    let batches = query(project_root, EXECUTIONS_SQL, &[]).await?;
    Ok(batches.iter().flat_map(execution_rows).collect())
}

/// Read one execution's threads, calling contexts, retained spans, and
/// errors. Four queries against one bound snapshot would be better; the
/// session binds per call today, which is honest but means the four reads
/// can straddle a commit. Each grain is internally consistent, and the UI
/// treats a span whose `call_path_id` is missing from the aggregates as
/// exactly that rather than inventing a parent.
pub async fn read_execution(
    project_root: &Path,
    execution_id: &str,
) -> Result<ExecutionTelemetry, TelemetryError> {
    let id = [execution_id.to_string()];
    let execution = query(project_root, EXECUTION_BY_ID_SQL, &id)
        .await?
        .iter()
        .flat_map(execution_rows)
        .next();
    let threads = query(project_root, THREADS_SQL, &id)
        .await?
        .iter()
        .flat_map(thread_rows)
        .collect();
    let call_paths = query(project_root, CALL_PATHS_SQL, &id)
        .await?
        .iter()
        .flat_map(call_path_rows)
        .collect();
    let calls = query(project_root, CALLS_SQL, &id)
        .await?
        .iter()
        .flat_map(call_rows)
        .collect();
    let errors = query(project_root, ERRORS_SQL, &id)
        .await?
        .iter()
        .flat_map(error_rows)
        .collect();
    Ok(ExecutionTelemetry {
        execution,
        threads,
        call_paths,
        calls,
        errors,
    })
}

/// Bind the store and run one server-authored statement.
///
/// `params` are substituted as quoted literals rather than bound values:
/// catalog v1 execution ids are opaque `baml_thread_1_…` tokens, and the
/// substitution rejects anything that is not one, so no client string ever
/// reaches the planner.
async fn query(
    project_root: &Path,
    sql: &str,
    params: &[String],
) -> Result<Vec<RecordBatch>, TelemetryError> {
    let store_root = store_root_for_project(project_root);
    if !store_root.exists() {
        return Err(TelemetryError::NoStore(store_root));
    }

    let sql = bind_params(sql, params)?;
    let mut budgets = QueryBudgets::unlimited();
    budgets.max_result_rows = MAX_ROWS;
    // Values are hydrated for every retained span, so these stop one
    // pathological capture from stalling the panel. A value over the cap
    // comes back unavailable with a reason, which the UI renders as such
    // rather than as "no value".
    budgets.max_value_bytes = MAX_VALUE_BYTES;
    budgets.max_decoded_bytes = MAX_DECODED_BYTES;
    budgets.max_wall = Some(std::time::Duration::from_secs(20));

    // The playground is a local, first-party reader: the internal profile is
    // what exposes store-debugging columns to it (catalog §4.1).
    let session = baml_query_profiles::profiles_session_with(
        &store_root,
        CatalogProfile::internal(),
        budgets,
        CancellationToken::new(),
    )
    .await
    .map_err(|err| TelemetryError::Query(err.to_string()))?;

    let mut execution = session
        .execute(&sql)
        .await
        .map_err(|(err, _)| TelemetryError::Query(err.to_string()))?;
    let mut batches = Vec::new();
    while let Some(batch) = execution.next_batch().await {
        batches.push(batch);
    }
    Ok(batches)
}

/// Substitute `$1`, `$2`, … with validated identifier literals.
fn bind_params(sql: &str, params: &[String]) -> Result<String, TelemetryError> {
    let mut out = sql.to_string();
    for (index, value) in params.iter().enumerate() {
        if !is_store_identifier(value) {
            return Err(TelemetryError::Query(format!(
                "not a store identifier: {value}"
            )));
        }
        out = out.replace(&format!("${}", index + 1), &format!("'{value}'"));
    }
    Ok(out)
}

/// Store identifiers are a prefix plus base64url with no padding
/// (`ids.rs` encodes them with `URL_SAFE_NO_PAD`), so the alphabet is
/// `A-Z a-z 0-9 - _`. That contains no quote, space, semicolon, or
/// backslash, which is what lets the substitution above be a literal
/// splice. Rejecting everything outside it keeps that true.
fn is_store_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// ---------------------------------------------------------------------------
// Arrow column readers
//
// Every getter is by name and returns None for an absent or null cell, so a
// catalog column that becomes nullable, or a relation that stops serving one,
// degrades to a missing field rather than a panic.
// ---------------------------------------------------------------------------

fn utf8(batch: &RecordBatch, column: &str, row: usize) -> Option<String> {
    let array = batch.column_by_name(column)?;
    let values = array.as_any().downcast_ref::<StringArray>()?;
    (!values.is_null(row)).then(|| values.value(row).to_string())
}

fn u64(batch: &RecordBatch, column: &str, row: usize) -> Option<u64> {
    let array = batch.column_by_name(column)?;
    if let Some(values) = array.as_any().downcast_ref::<UInt64Array>() {
        return (!values.is_null(row)).then(|| values.value(row));
    }
    let values = array.as_any().downcast_ref::<UInt32Array>()?;
    (!values.is_null(row)).then(|| u64::from(values.value(row)))
}

fn u32(batch: &RecordBatch, column: &str, row: usize) -> Option<u32> {
    let array = batch.column_by_name(column)?;
    let values = array.as_any().downcast_ref::<UInt32Array>()?;
    (!values.is_null(row)).then(|| values.value(row))
}

fn boolean(batch: &RecordBatch, column: &str, row: usize) -> Option<bool> {
    let array = batch.column_by_name(column)?;
    let values = array.as_any().downcast_ref::<BooleanArray>()?;
    (!values.is_null(row)).then(|| values.value(row))
}

/// Timestamps arrive as UTC nanoseconds; the UI works in epoch millis.
fn timestamp_ms(batch: &RecordBatch, column: &str, row: usize) -> Option<i64> {
    let array = batch.column_by_name(column)?;
    let values = array.as_any().downcast_ref::<TimestampNanosecondArray>()?;
    (!values.is_null(row)).then(|| values.value(row) / 1_000_000)
}

fn utf8_list(batch: &RecordBatch, column: &str, row: usize) -> Vec<String> {
    let Some(array) = batch.column_by_name(column) else {
        return Vec::new();
    };
    let Some(values) = array
        .as_any()
        .downcast_ref::<datafusion::arrow::array::ListArray>()
    else {
        return Vec::new();
    };
    if values.is_null(row) {
        return Vec::new();
    }
    let item = values.value(row);
    let Some(strings) = item.as_any().downcast_ref::<StringArray>() else {
        return Vec::new();
    };
    (0..strings.len())
        .filter(|index| !strings.is_null(*index))
        .map(|index| strings.value(index).to_string())
        .collect()
}

fn execution_rows(batch: &RecordBatch) -> Vec<ExecutionRow> {
    (0..batch.num_rows())
        .filter_map(|row| {
            Some(ExecutionRow {
                execution_id: utf8(batch, "execution_id", row)?,
                entry_fqn: utf8(batch, "entry_fqn", row),
                source_label: utf8(batch, "source_label", row),
                revision_id: utf8(batch, "revision_id", row),
                status: utf8(batch, "status", row),
                index_state: utf8(batch, "index_state", row),
                value_state: utf8(batch, "value_state", row),
                started_at_ms: timestamp_ms(batch, "started_at", row),
                duration_ns: u64(batch, "duration_ns", row),
                total_calls: u64(batch, "total_calls", row),
                total_errors: u64(batch, "total_errors", row),
                calls_retained: u64(batch, "calls_retained", row),
                threads_total: u64(batch, "threads_total", row),
            })
        })
        .collect()
}

fn thread_rows(batch: &RecordBatch) -> Vec<ThreadRow> {
    (0..batch.num_rows())
        .filter_map(|row| {
            Some(ThreadRow {
                thread_id: utf8(batch, "thread_id", row)?,
                parent_thread_id: utf8(batch, "parent_thread_id", row),
                spawn_call_id: utf8(batch, "spawn_call_id", row),
                spawn_fqn: utf8(batch, "spawn_fqn", row),
                spawn_site_file: utf8(batch, "spawn_site_file", row),
                spawn_site_line: u32(batch, "spawn_site_line", row),
                name: utf8(batch, "name", row),
                kind: utf8(batch, "kind", row),
                started_ns: u64(batch, "started_ns", row),
                ended_ns: u64(batch, "ended_ns", row),
                end_status: utf8(batch, "end_status", row),
            })
        })
        .collect()
}

fn call_path_rows(batch: &RecordBatch) -> Vec<CallPathRow> {
    (0..batch.num_rows())
        .filter_map(|row| {
            Some(CallPathRow {
                call_path_id: utf8(batch, "call_path_id", row)?,
                parent_call_path_id: utf8(batch, "parent_call_path_id", row),
                depth: u32(batch, "depth", row),
                fqn: utf8(batch, "fqn", row),
                kind: utf8(batch, "kind", row),
                origin: utf8(batch, "origin", row),
                edge_kind: utf8(batch, "edge_kind", row),
                call_site_file: utf8(batch, "call_site_file", row),
                call_site_line: u32(batch, "call_site_line", row),
                call_site_start: u32(batch, "call_site_start", row),
                call_site_end: u32(batch, "call_site_end", row),
                calls_started: u64(batch, "calls_started", row),
                calls_selected: u64(batch, "calls_selected", row),
                completed_ok: u64(batch, "completed_ok", row),
                completed_error: u64(batch, "completed_error", row),
                completed_cancelled: u64(batch, "completed_cancelled", row),
                inclusive_ns: u64(batch, "inclusive_ns", row),
                direct_child_ns: u64(batch, "direct_child_ns", row),
                await_ns: u64(batch, "await_ns", row),
                self_ns: u64(batch, "self_ns", row),
                timing_complete: boolean(batch, "timing_complete", row),
                overflow_reason: utf8(batch, "overflow_reason", row),
            })
        })
        .collect()
}

fn call_rows(batch: &RecordBatch) -> Vec<CallRow> {
    (0..batch.num_rows())
        .filter_map(|row| {
            Some(CallRow {
                call_id: utf8(batch, "call_id", row)?,
                parent_call_id: utf8(batch, "parent_call_id", row),
                thread_id: utf8(batch, "thread_id", row),
                call_path_id: utf8(batch, "call_path_id", row),
                fqn: utf8(batch, "fqn", row),
                kind: utf8(batch, "kind", row),
                edge_kind: utf8(batch, "edge_kind", row),
                call_site_file: utf8(batch, "call_site_file", row),
                call_site_line: u32(batch, "call_site_line", row),
                started_ns: u64(batch, "started_ns", row),
                ended_ns: u64(batch, "ended_ns", row),
                duration_ns: u64(batch, "duration_ns", row),
                status: utf8(batch, "status", row),
                selection_reasons: utf8_list(batch, "selection_reasons", row),
                args_state: utf8(batch, "args_state", row),
                output_state: utf8(batch, "output_state", row),
                error_state: utf8(batch, "error_state", row),
                args_cid: utf8(batch, "args_cid", row),
                output_cid: utf8(batch, "output_cid", row),
                error_cid: utf8(batch, "error_cid", row),
                error_id: utf8(batch, "error_id", row),
                args: utf8(batch, "args", row),
                output: utf8(batch, "output", row),
                error: utf8(batch, "error", row),
            })
        })
        .collect()
}

fn error_rows(batch: &RecordBatch) -> Vec<ErrorRow> {
    (0..batch.num_rows())
        .filter_map(|row| {
            Some(ErrorRow {
                error_id: utf8(batch, "error_id", row)?,
                throw_call_id: utf8(batch, "throw_call_id", row),
                throw_thread_id: utf8(batch, "throw_thread_id", row),
                throw_call_path_id: utf8(batch, "throw_call_path_id", row),
                throw_fqn: utf8(batch, "throw_fqn", row),
                throw_site_file: utf8(batch, "throw_site_file", row),
                throw_site_line: u32(batch, "throw_site_line", row),
                kind: utf8(batch, "kind", row),
                source: utf8(batch, "source", row),
                value_state: utf8(batch, "value_state", row),
                value_cid: utf8(batch, "value_cid", row),
                stack_complete: boolean(batch, "stack_complete", row),
                stack: utf8_list(batch, "stack", row),
                value: utf8(batch, "value", row),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_wire_identifiers() {
        assert!(is_store_identifier("baml_thread_1_00112233"));
        assert!(is_store_identifier("abc123_DEF"));
    }

    #[test]
    fn accepts_base64url_identifiers_containing_a_hyphen() {
        // Ids are base64url with no padding, so roughly one id in two
        // carries a `-` or `_`. An alphanumeric-only check passes on the
        // ids that happen to avoid them and rejects the rest at random.
        assert!(is_store_identifier(
            "baml_thread_1_Ad-UqB_abUgysoQ5s1CT3qMAAAAAAAAAQAAAAAAAAAG"
        ));
        assert!(is_store_identifier("-_-_"));
    }

    #[test]
    fn rejects_anything_that_could_carry_sql() {
        assert!(!is_store_identifier(""));
        assert!(!is_store_identifier("a'; DROP TABLE calls_v1; --"));
        assert!(!is_store_identifier("has space"));
        assert!(!is_store_identifier("quote'inside"));
        assert!(!is_store_identifier(&"x".repeat(129)));
    }

    #[test]
    fn binds_positional_params_as_literals() {
        let sql = bind_params("SELECT 1 WHERE a = $1 AND b = $1", &["abc".to_string()])
            .expect("valid identifier");
        assert_eq!(sql, "SELECT 1 WHERE a = 'abc' AND b = 'abc'");
    }

    #[test]
    fn refuses_to_bind_a_non_identifier() {
        let err = bind_params("SELECT $1", &["'; --".to_string()]).unwrap_err();
        assert!(matches!(err, TelemetryError::Query(_)));
    }
}
