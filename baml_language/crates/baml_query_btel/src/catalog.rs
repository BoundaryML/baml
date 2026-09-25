//! The public relations. A prototype contract, not the final catalog: each
//! relation is a view over the internal tables, created as a TEMP view on
//! every query connection so its definition always matches this binary.
//!
//! Identifiers render as text and are unique across recordings:
//! `recording_id` is 32 hex digits; executions, threads and calls share one
//! node namespace, `<recording_id>:<n>` (an execution's id is its root
//! thread's id); call paths are `<recording_id>:p<n>`, functions
//! `<recording_id>:f<n>` and clock epochs `<recording_id>:c<n>`. Durations
//! are nanoseconds, NULL when the clock evidence cannot support them
//! (`timing_state` says why). `args`, `output` and `error` are BAML values:
//! navigate them with `['key']` and `[index]`.
//!
//! Counting rules: `call_path_stats` and `function_stats` count every
//! completed invocation the recording aggregated; `calls` holds only
//! individually retained invocations. Never add the two.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Column {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub sql_type: &'static str,
    /// A BAML value column: supports bracket navigation, rendered on output.
    pub value: bool,
    pub doc: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Relation {
    pub name: &'static str,
    pub doc: &'static str,
    pub columns: &'static [Column],
    #[serde(skip)]
    pub view: &'static str,
}

impl Relation {
    /// The `CREATE TEMP VIEW` statement for this relation.
    pub fn create_sql(&self) -> String {
        if self.name == CAPABILITIES.name {
            capabilities_view()
        } else {
            self.view.to_owned()
        }
    }
}

const fn col(name: &'static str, sql_type: &'static str, doc: &'static str) -> Column {
    Column {
        name,
        sql_type,
        value: false,
        doc,
    }
}
const fn value(name: &'static str, doc: &'static str) -> Column {
    Column {
        name,
        sql_type: "baml_value",
        value: true,
        doc,
    }
}

// These counts cover completed invocations in the indexed aggregate prefix.
const OK_CALLS: Column = col(
    "ok_calls",
    "integer",
    "successful completed invocations; NULL when outcome evidence is missing, partial, invalid or the count overflows",
);
const ERRORED_CALLS: Column = col(
    "errored_calls",
    "integer",
    "errored completed invocations, including timing-only calls; NULL when unavailable",
);
const CANCELLED_CALLS: Column = col(
    "cancelled_calls",
    "integer",
    "cancelled completed invocations; NULL when unavailable",
);
const OUTCOME_STATE: Column = col(
    "outcome_state",
    "text",
    "recorded (all indexed completions have outcomes), none_observed, not_recorded (old deltas), partial (mixed old/new), invalid, overflow; independent of clock validity",
);

pub const RECORDINGS: Relation = Relation {
    name: "recordings",
    doc: "One row per recording directory. The indexed prefix is what every other relation answers from.",
    columns: &[
        col("recording_id", "text", "32 hex digits"),
        col(
            "state",
            "text",
            "summary: unsealed (no end marker; the prefix may grow), sealed (every file through the end marker applied), gap, invalid_file, files_after_end",
        ),
        col(
            "seal_state",
            "text",
            "sealed when an end marker was indexed, else unsealed. An end beyond a missing file is not indexed until that file arrives. Sealed is not proof every capture or cloud upload arrived; unsealed is not proof the program is still running",
        ),
        col(
            "prefix_state",
            "text",
            "complete_prefix (every file up to indexed_sequence applied), gap, invalid_file, files_after_end",
        ),
        col(
            "indexed_sequence",
            "integer",
            "last file of the contiguous applied prefix",
        ),
        col(
            "observed_sequence",
            "integer",
            "highest completed file found on disk",
        ),
        col(
            "terminal_sequence",
            "integer",
            "file carrying the end marker, if applied",
        ),
        col(
            "blocked_sequence",
            "integer",
            "first file that could not be applied",
        ),
        col(
            "partial_files",
            "integer",
            "unfinished .part files seen (writes in progress or abandoned); never indexed",
        ),
        col("indexed_bytes", "integer", "bytes of applied files"),
        col(
            "source_snapshot_id",
            "text",
            "source fingerprint recorded by the producer, if any; provenance, not identity",
        ),
        col(
            "format_minor",
            "integer",
            "highest recording format minor version applied",
        ),
    ],
    view: "
CREATE TEMP VIEW recordings AS
SELECT lower(hex(r.recording_id)) AS recording_id,
  CASE
    WHEN r.blocked_reason = 'invalid' THEN 'invalid_file'
    WHEN r.blocked_reason = 'missing' THEN 'gap'
    WHEN r.files_after_end > 0 THEN 'files_after_end'
    WHEN r.terminal_sequence IS NOT NULL AND r.terminal_sequence = r.indexed_sequence THEN 'sealed'
    ELSE 'unsealed'
  END AS state,
  CASE WHEN r.terminal_sequence IS NOT NULL THEN 'sealed' ELSE 'unsealed' END AS seal_state,
  CASE
    WHEN r.blocked_reason = 'invalid' THEN 'invalid_file'
    WHEN r.blocked_reason = 'missing' THEN 'gap'
    WHEN r.files_after_end > 0 THEN 'files_after_end'
    ELSE 'complete_prefix'
  END AS prefix_state,
  r.indexed_sequence, r.observed_sequence, r.terminal_sequence, r.blocked_sequence,
  r.partial_files, r.indexed_bytes,
  lower(hex(r.source_snapshot_id)) AS source_snapshot_id, r.format_minor
FROM main.recording r",
};

pub const THREADS: Relation = Relation {
    name: "threads",
    doc: "Logical BAML threads (tasks, not OS threads): each execution's root and every spawned thread, with the telemetry hierarchy. Includes threads that are referenced but not yet defined.",
    columns: &[
        col("thread_id", "text", "<recording_id>:<n>"),
        col("recording_id", "text", ""),
        col(
            "execution_id",
            "text",
            "the root thread this thread belongs to; NULL while its ancestry is unresolved",
        ),
        col(
            "definition_state",
            "text",
            "resolved, unresolved (referenced, no definition indexed), conflicted",
        ),
        col(
            "structure_state",
            "text",
            "resolved (execution known), unresolved, conflicted",
        ),
        col(
            "parent_node_id",
            "text",
            "recorded parent: the spawning thread or retained call; NULL for a root",
        ),
        col(
            "parent_node_kind",
            "text",
            "thread or call; NULL for a root or when the parent is not indexed",
        ),
        col(
            "parent_thread_id",
            "text",
            "the thread that spawned this one, when resolvable",
        ),
        col(
            "spawn_call_id",
            "text",
            "the retained call that was running when this thread was spawned, if the parent is a call",
        ),
        col(
            "spawn_call_path_id",
            "text",
            "the spawn edge's calling context",
        ),
        col(
            "spawn_function_id",
            "text",
            "the spawned function (callee of the spawn edge)",
        ),
        col("spawn_fqn", "text", "its recorded name"),
        col(
            "spawn_site_state",
            "text",
            "resolved (the spawn expression in the spawning function), not_spawned (a root), path_unresolved, or why the spawn path's site is unavailable (call_paths.call_site_state)",
        ),
        col(
            "spawn_site_file",
            "text",
            "file of the spawn expression, as recorded",
        ),
        col(
            "spawn_site_line",
            "integer",
            "one-based line of the spawn expression",
        ),
        col("spawn_site_start", "integer", ""),
        col("spawn_site_end", "integer", ""),
        col(
            "kind",
            "text",
            "root or spawn; NULL until the definition is indexed",
        ),
        col(
            "is_root",
            "integer",
            "1 for execution roots, 0 for spawned threads, NULL if unknown",
        ),
        col("clock_epoch_id", "text", "the clock its ticks belong to"),
        col("started_ticks", "text", "raw recorded start tick"),
        col("ended_ticks", "text", "raw recorded completion tick"),
        col("started_at", "text", "UTC estimate, RFC 3339"),
        col("ended_at", "text", "UTC estimate, RFC 3339"),
        col(
            "started_unix_ns",
            "integer",
            "UTC estimate in Unix nanoseconds",
        ),
        col(
            "ended_unix_ns",
            "integer",
            "UTC estimate in Unix nanoseconds",
        ),
        col(
            "start_offset_ns",
            "integer",
            "nanoseconds from its execution's start",
        ),
        col(
            "end_offset_ns",
            "integer",
            "nanoseconds from its execution's start to its completion",
        ),
        col(
            "duration_ns",
            "integer",
            "wall-clock lifetime; not CPU time. NULL unless timing_state is valid",
        ),
        col(
            "timing_state",
            "text",
            "valid, incomplete, unknown_clock, conflicted, invalidated_*, backward, overflow",
        ),
        col(
            "end_status",
            "text",
            "ok, errored or cancelled when a completion is indexed",
        ),
        col(
            "completion_state",
            "text",
            "present, missing (no completion indexed: not proof it is still running), conflicted",
        ),
    ],
    view: "
CREATE TEMP VIEW threads AS
SELECT __btel_pubid(r.recording_id, t.thread_id) AS thread_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  CASE WHEN t.defined = 0 THEN 'unresolved' WHEN t.conflict = 1 THEN 'conflicted'
    ELSE 'resolved' END AS definition_state,
  CASE WHEN t.conflict = 1 THEN 'conflicted' WHEN t.root_id IS NOT NULL THEN 'resolved'
    ELSE 'unresolved' END AS structure_state,
  __btel_pubid(r.recording_id, t.parent_id) AS parent_node_id,
  CASE WHEN t.parent_id IS NULL THEN NULL WHEN pt.rec IS NOT NULL THEN 'thread'
    WHEN pc.rec IS NOT NULL THEN 'call' END AS parent_node_kind,
  __btel_pubid(r.recording_id, COALESCE(pt.thread_id, pc.thread_id)) AS parent_thread_id,
  __btel_pubid(r.recording_id, pc.call_id) AS spawn_call_id,
  __btel_sid(r.recording_id, 'p', NULLIF(t.spawn_call_path_id, 0)) AS spawn_call_path_id,
  __btel_sid(r.recording_id, 'f', sp.callee_function_id) AS spawn_function_id,
  sf.fqn AS spawn_fqn,
  CASE WHEN t.defined = 1 AND t.parent_id IS NULL THEN 'not_spawned'
    WHEN sp.rec IS NULL THEN 'path_unresolved' ELSE sp.site_state END AS spawn_site_state,
  sp.site_file AS spawn_site_file, sp.site_line AS spawn_site_line,
  sp.site_start AS spawn_site_start, sp.site_end AS spawn_site_end,
  CASE WHEN t.defined = 0 THEN NULL WHEN t.parent_id IS NULL THEN 'root' ELSE 'spawn' END AS kind,
  CASE WHEN t.defined = 0 THEN NULL WHEN t.parent_id IS NULL THEN 1 ELSE 0 END AS is_root,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  CAST(t.started_ticks AS TEXT) AS started_ticks,
  CAST(t.completed_ticks AS TEXT) AS ended_ticks,
  __btel_utc(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_at,
  __btel_utc(t.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_at,
  __btel_unix_ns(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_unix_ns,
  __btel_unix_ns(t.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_unix_ns,
  IIF(root.epoch_id = t.epoch_id, __btel_duration(root.started_ticks, t.started_ticks,
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)), NULL)
    AS start_offset_ns,
  IIF(root.epoch_id = t.epoch_id, __btel_duration(root.started_ticks, t.completed_ticks,
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)), NULL)
    AS end_offset_ns,
  __btel_duration(t.started_ticks, t.completed_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration_ns,
  __btel_timing(t.started_ticks, t.completed_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS timing_state,
  CASE t.outcome WHEN 1 THEN 'ok' WHEN 2 THEN 'errored' WHEN 3 THEN 'cancelled' END AS end_status,
  CASE WHEN t.completion_conflict = 1 THEN 'conflicted' WHEN t.outcome IS NOT NULL THEN 'present'
    ELSE 'missing' END AS completion_state
FROM main.thread t
JOIN main.recording r ON r.rec = t.rec
LEFT JOIN main.thread pt ON pt.rec = t.rec AND pt.thread_id = t.parent_id
LEFT JOIN main.call pc ON pc.rec = t.rec AND pc.call_id = t.parent_id
LEFT JOIN main.thread root ON root.rec = t.rec AND root.thread_id = t.root_id
LEFT JOIN main.call_path sp ON sp.rec = t.rec AND sp.call_path_id = t.spawn_call_path_id
  AND sp.defined = 1
LEFT JOIN main.function_def sf ON sf.rec = sp.rec AND sf.function_id = sp.callee_function_id
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = t.rec AND s.epoch_id = t.epoch_id",
};

pub const EXECUTIONS: Relation = Relation {
    name: "executions",
    doc: "Host invocations: one per root thread whose definition is indexed. A recording can contain many. Threads with an unresolved parent never appear here as roots.",
    columns: &[
        col("execution_id", "text", "<recording_id>:<root thread id>"),
        col("recording_id", "text", ""),
        col(
            "thread_id",
            "text",
            "the root thread; equal to execution_id",
        ),
        col(
            "entry_function_id",
            "text",
            "function the root thread entered, when exactly one is recorded",
        ),
        col("entry_fqn", "text", "its recorded name"),
        col(
            "entry_state",
            "text",
            "resolved, unresolved (no top-level call recorded yet), ambiguous",
        ),
        col(
            "status",
            "text",
            "ok, errored, cancelled, incomplete (no root completion indexed; in a sealed recording none will arrive), conflicted",
        ),
        col(
            "completion_state",
            "text",
            "present, missing, conflicted (see threads)",
        ),
        col("clock_epoch_id", "text", ""),
        col("started_at", "text", "UTC estimate, RFC 3339"),
        col(
            "started_at_ms",
            "integer",
            "the same instant in Unix milliseconds",
        ),
        col(
            "started_unix_ns",
            "integer",
            "UTC estimate in Unix nanoseconds",
        ),
        col("ended_at", "text", "UTC estimate, RFC 3339"),
        col(
            "ended_unix_ns",
            "integer",
            "UTC estimate in Unix nanoseconds",
        ),
        col(
            "duration_ns",
            "integer",
            "root thread lifetime; NULL unless timing_state is valid",
        ),
        col(
            "timing_state",
            "text",
            "valid, incomplete, unknown_clock, conflicted, invalidated_*, backward, overflow",
        ),
        col(
            "completed_calls",
            "integer",
            "completed invocations aggregated for this execution's threads (the whole recorded population, not invocation starts)",
        ),
        col(
            "calls_retained",
            "integer",
            "individually retained calls in this execution",
        ),
        col("retained_calls", "integer", "alias of calls_retained"),
        col(
            "retained_ok_calls",
            "integer",
            "retained calls that completed ok (retained only: not a population count)",
        ),
        col(
            "retained_errored_calls",
            "integer",
            "retained calls that errored",
        ),
        col(
            "retained_cancelled_calls",
            "integer",
            "retained calls that were cancelled",
        ),
        col(
            "retained_incomplete_calls",
            "integer",
            "retained calls with no completion indexed",
        ),
        col(
            "threads_total",
            "integer",
            "threads attributed to this execution so far, including the root",
        ),
        col("threads", "integer", "alias of threads_total"),
        col(
            "aggregate_state",
            "text",
            "observed_prefix, or overflow when completed_calls does not fit",
        ),
    ],
    view: "
CREATE TEMP VIEW executions AS
SELECT __btel_pubid(r.recording_id, t.thread_id) AS execution_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, t.thread_id) AS thread_id,
  __btel_sid(r.recording_id, 'f',
    (SELECT CASE WHEN COUNT(DISTINCT p.callee_function_id) = 1 THEN MIN(p.callee_function_id) END
     FROM main.call_path p
     WHERE p.rec = t.rec AND p.thread_id = t.thread_id AND p.parent_call_path_id = 0
       AND p.edge = 1)) AS entry_function_id,
  (SELECT CASE WHEN COUNT(DISTINCT p.callee_function_id) = 1 THEN MIN(f.fqn) END
     FROM main.call_path p
     LEFT JOIN main.function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
     WHERE p.rec = t.rec AND p.thread_id = t.thread_id
       AND p.parent_call_path_id = 0 AND p.edge = 1) AS entry_fqn,
  (SELECT CASE COUNT(DISTINCT p.callee_function_id) WHEN 0 THEN 'unresolved' WHEN 1 THEN 'resolved'
       ELSE 'ambiguous' END
     FROM main.call_path p
     WHERE p.rec = t.rec AND p.thread_id = t.thread_id AND p.parent_call_path_id = 0
       AND p.edge = 1) AS entry_state,
  CASE WHEN t.completion_conflict = 1 THEN 'conflicted'
    ELSE CASE t.outcome WHEN 1 THEN 'ok' WHEN 2 THEN 'errored' WHEN 3 THEN 'cancelled'
      ELSE 'incomplete' END END AS status,
  CASE WHEN t.completion_conflict = 1 THEN 'conflicted' WHEN t.outcome IS NOT NULL THEN 'present'
    ELSE 'missing' END AS completion_state,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  __btel_utc(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_at,
  __btel_unix_ms(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_at_ms,
  __btel_unix_ns(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_unix_ns,
  __btel_utc(t.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_at,
  __btel_unix_ns(t.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_unix_ns,
  __btel_duration(t.started_ticks, t.completed_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration_ns,
  __btel_timing(t.started_ticks, t.completed_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS timing_state,
  (SELECT __btel_sum(a.call_count) FROM main.thread d
     CROSS JOIN main.call_path p ON p.rec = d.rec AND p.thread_id = d.thread_id
     CROSS JOIN main.aggregate a ON a.rec = p.rec
       AND a.node BETWEEN p.call_path_id * 2 AND p.call_path_id * 2 + 1
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS completed_calls,
  -- CROSS JOIN fixes the order: this execution's threads, then their calls.
  (SELECT COUNT(*) FROM main.thread d CROSS JOIN main.call k
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS calls_retained,
  (SELECT COUNT(*) FROM main.thread d CROSS JOIN main.call k
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS retained_calls,
  (SELECT COUNT(IIF(k.outcome = 1 AND k.conflict = 0, 1, NULL)) FROM main.thread d
     CROSS JOIN main.call k INDEXED BY call_by_thread
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS retained_ok_calls,
  (SELECT COUNT(IIF(k.outcome = 2 AND k.conflict = 0, 1, NULL)) FROM main.thread d
     CROSS JOIN main.call k INDEXED BY call_by_thread
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS retained_errored_calls,
  (SELECT COUNT(IIF(k.outcome = 3 AND k.conflict = 0, 1, NULL)) FROM main.thread d
     CROSS JOIN main.call k INDEXED BY call_by_thread
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS retained_cancelled_calls,
  (SELECT COUNT(IIF(k.outcome IS NULL AND k.conflict = 0, 1, NULL)) FROM main.thread d
     CROSS JOIN main.call k INDEXED BY call_by_thread
     ON k.rec = d.rec AND k.thread_id = d.thread_id
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) AS retained_incomplete_calls,
  (SELECT COUNT(*) FROM main.thread d WHERE d.rec = t.rec AND d.root_id = t.thread_id)
    AS threads_total,
  (SELECT COUNT(*) FROM main.thread d WHERE d.rec = t.rec AND d.root_id = t.thread_id)
    AS threads,
  CASE WHEN (SELECT __btel_sum(a.call_count) FROM main.thread d
     CROSS JOIN main.call_path p ON p.rec = d.rec AND p.thread_id = d.thread_id
     CROSS JOIN main.aggregate a ON a.rec = p.rec
       AND a.node BETWEEN p.call_path_id * 2 AND p.call_path_id * 2 + 1
     WHERE d.rec = t.rec AND d.root_id = t.thread_id) IS NULL THEN 'overflow'
    ELSE 'observed_prefix' END AS aggregate_state
FROM main.thread t
JOIN main.recording r ON r.rec = t.rec
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = t.rec AND s.epoch_id = t.epoch_id
WHERE t.defined = 1 AND t.parent_id IS NULL",
};

const CALL_COLUMNS: &[Column] = &[
    col("call_id", "text", "<recording_id>:<call telemetry id>"),
    col(
        "execution_id",
        "text",
        "NULL while the thread's execution is not yet resolvable",
    ),
    col("recording_id", "text", ""),
    col("thread_id", "text", "the thread the call ran on"),
    col(
        "parent_node_id",
        "text",
        "recorded telemetry parent: a retained call or the thread. Timed-only calls in between are not nodes",
    ),
    col("parent_id", "text", "alias of parent_node_id"),
    col(
        "parent_node_kind",
        "text",
        "call or thread; NULL when the parent is not indexed",
    ),
    col(
        "parent_call_id",
        "text",
        "the parent when it is a retained call",
    ),
    col("call_path_id", "text", "calling context (see call_paths)"),
    col(
        "reentry",
        "integer",
        "1 for a direct recursive re-entry collapsed onto its caller's path; NULL before completion",
    ),
    col("function_id", "text", ""),
    col("fqn", "text", "NULL when no function metadata was recorded"),
    col("kind", "text", "bytecode, sysop, native, native_unresolved"),
    col(
        "call_site_state",
        "text",
        "resolved (the call expression this invocation entered from), recursive_reentry (a direct recursive call: its own site is not recorded; see call_paths for the context's entry site), reentry_unknown (no completion yet), path_unresolved, or why the path's site is unavailable (call_paths.call_site_state)",
    ),
    col(
        "call_site_file",
        "text",
        "file of the call expression, as recorded; today's file may differ",
    ),
    col(
        "call_site_line",
        "integer",
        "one-based line of the call expression",
    ),
    col(
        "call_site_start",
        "integer",
        "byte offset where the call expression starts",
    ),
    col("call_site_end", "integer", "byte offset where it ends"),
    col(
        "structure_state",
        "text",
        "resolved (execution and path known), unresolved, conflicted",
    ),
    col(
        "evidence_state",
        "text",
        "paired (announcement + completion), announcement_only, completion_only, late_completion (promoted after it finished), conflicted",
    ),
    col(
        "announcement_required",
        "integer",
        "1 when the completion says an input announcement carries its captured args; NULL before completion",
    ),
    col(
        "status",
        "text",
        "ok, errored, cancelled, incomplete (no completion indexed), conflicted",
    ),
    col(
        "error_raise_id",
        "text",
        "the raise that unwound this call (see error_raises); NULL unless error_link_state is linked",
    ),
    col(
        "error_occurrence_id",
        "text",
        "that raise's occurrence; NULL when its origin is ambiguous or unresolved",
    ),
    col(
        "error_link_state",
        "text",
        "linked, unlinked (failed with no raise recorded, e.g. ended by the engine), not_recorded (a recording without error evidence), not_applicable (did not fail)",
    ),
    col("clock_epoch_id", "text", ""),
    col("started_ticks", "text", "raw recorded entry tick"),
    col("ended_ticks", "text", "raw recorded exit tick"),
    col("self_await_ticks", "text", "raw recorded await ticks"),
    col("started_at", "text", "UTC estimate, RFC 3339"),
    col("ended_at", "text", "UTC estimate, RFC 3339"),
    col(
        "started_unix_ns",
        "integer",
        "UTC estimate in Unix nanoseconds",
    ),
    col(
        "ended_unix_ns",
        "integer",
        "UTC estimate in Unix nanoseconds",
    ),
    col(
        "start_offset_ns",
        "integer",
        "nanoseconds from the execution's start; NULL unless its clock is valid",
    ),
    col(
        "end_offset_ns",
        "integer",
        "nanoseconds from the execution's start to this call's exit",
    ),
    col(
        "duration_ns",
        "integer",
        "this invocation's inclusive duration; NULL unless timing_state is valid",
    ),
    col(
        "self_await_ns",
        "integer",
        "time this call's own frame spent awaiting; not CPU time",
    ),
    col(
        "timing_state",
        "text",
        "valid, incomplete, unknown_clock, conflicted, invalidated_*, backward, overflow",
    ),
    col(
        "args_state",
        "text",
        "reference (a capture id was recorded), pending (its announcement is not indexed yet), not_recorded, conflicted",
    ),
    col(
        "output_state",
        "text",
        "reference, not_recorded, not_applicable (the call did not return ok), pending (no completion yet), conflicted",
    ),
    col(
        "error_state",
        "text",
        "reference, not_recorded, not_applicable (the call did not error), pending, conflicted",
    ),
    col(
        "value_state",
        "text",
        "captured or none: whether an output or error value was recorded (older column)",
    ),
    col("args_cid", "text", "content id of the captured inputs"),
    col("output_cid", "text", "content id of the captured output"),
    col("error_cid", "text", "content id of the captured error"),
    col("args_cas_id", "text", "alias of args_cid"),
    col(
        "value_cas_id",
        "text",
        "content id of the captured output or error (older column)",
    ),
    value(
        "args",
        "captured inputs by recorded parameter name or position: args['customer']['age'], args[0]",
    ),
    value(
        "output",
        "captured return value of an ok call: output['items'][0]['name']",
    ),
    value("error", "captured error value of an errored call"),
];

pub const CALLS: Relation = Relation {
    name: "calls",
    doc: "Individually retained calls (by capture policy, e.g. LLM functions, or promoted after finishing). Never all calls: call_path_stats and function_stats count those.",
    columns: CALL_COLUMNS,
    view: "
CREATE TEMP VIEW calls AS
SELECT __btel_pubid(r.recording_id, c.call_id) AS call_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, c.thread_id) AS thread_id,
  __btel_pubid(r.recording_id, c.parent_id) AS parent_node_id,
  __btel_pubid(r.recording_id, c.parent_id) AS parent_id,
  CASE WHEN pc.rec IS NOT NULL THEN 'call' WHEN pt.rec IS NOT NULL THEN 'thread' END
    AS parent_node_kind,
  __btel_pubid(r.recording_id, pc.call_id) AS parent_call_id,
  __btel_sid(r.recording_id, 'p', c.call_path_id) AS call_path_id,
  c.reentry AS reentry,
  __btel_sid(r.recording_id, 'f', p.callee_function_id) AS function_id,
  f.fqn AS fqn,
  f.kind AS kind,
  CASE WHEN p.rec IS NULL OR p.defined = 0 THEN 'path_unresolved'
    WHEN c.reentry = 1 THEN 'recursive_reentry'
    WHEN c.reentry IS NULL THEN 'reentry_unknown'
    ELSE p.site_state END AS call_site_state,
  IIF(c.reentry = 0, p.site_file, NULL) AS call_site_file,
  IIF(c.reentry = 0, p.site_line, NULL) AS call_site_line,
  IIF(c.reentry = 0, p.site_start, NULL) AS call_site_start,
  IIF(c.reentry = 0, p.site_end, NULL) AS call_site_end,
  CASE WHEN c.conflict > 0 OR p.conflict = 1 THEN 'conflicted'
    WHEN t.root_id IS NOT NULL AND p.defined = 1 THEN 'resolved' ELSE 'unresolved' END
    AS structure_state,
  CASE WHEN c.conflict > 0 THEN 'conflicted'
    WHEN c.late = 1 THEN 'late_completion'
    WHEN c.announced_sequence IS NOT NULL AND c.completed_sequence IS NOT NULL THEN 'paired'
    WHEN c.announced_sequence IS NOT NULL THEN 'announcement_only'
    ELSE 'completion_only' END AS evidence_state,
  c.needs_announcement AS announcement_required,
  CASE WHEN c.conflict > 0 THEN 'conflicted'
    ELSE CASE c.outcome WHEN 1 THEN 'ok' WHEN 2 THEN 'errored' WHEN 3 THEN 'cancelled'
      ELSE 'incomplete' END END AS status,
  __btel_pubid(r.recording_id, el.raise_id) AS error_raise_id,
  __btel_pubid(r.recording_id, CASE er.origin_state WHEN 1 THEN er.raise_id
    WHEN 2 THEN er.origin_raise_id END) AS error_occurrence_id,
  CASE WHEN c.outcome IS NULL OR c.outcome = 1 THEN 'not_applicable'
    WHEN el.rec IS NOT NULL THEN 'linked'
    WHEN r.format_minor < 2 THEN 'not_recorded'
    ELSE 'unlinked' END AS error_link_state,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  CAST(c.entered_ticks AS TEXT) AS started_ticks,
  CAST(c.exited_ticks AS TEXT) AS ended_ticks,
  CAST(c.self_await_ticks AS TEXT) AS self_await_ticks,
  __btel_utc(c.entered_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_at,
  __btel_utc(c.exited_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_at,
  __btel_unix_ns(c.entered_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS started_unix_ns,
  __btel_unix_ns(c.exited_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS ended_unix_ns,
  IIF(root.epoch_id = t.epoch_id, __btel_duration(root.started_ticks, c.entered_ticks,
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)), NULL)
    AS start_offset_ns,
  IIF(root.epoch_id = t.epoch_id, __btel_duration(root.started_ticks, c.exited_ticks,
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)), NULL)
    AS end_offset_ns,
  __btel_duration(c.entered_ticks, c.exited_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration_ns,
  __btel_total_ns(c.self_await_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS self_await_ns,
  __btel_timing(c.entered_ticks, c.exited_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS timing_state,
  CASE WHEN c.conflict > 0 THEN 'conflicted'
    WHEN c.inputs_cas IS NOT NULL THEN 'reference'
    WHEN c.needs_announcement = 1 AND c.announced_sequence IS NULL THEN 'pending'
    ELSE 'not_recorded' END AS args_state,
  CASE WHEN c.conflict > 0 THEN 'conflicted' WHEN c.outcome IS NULL THEN 'pending'
    WHEN c.outcome != 1 THEN 'not_applicable'
    WHEN c.value_cas IS NOT NULL THEN 'reference' ELSE 'not_recorded' END AS output_state,
  CASE WHEN c.conflict > 0 THEN 'conflicted' WHEN c.outcome IS NULL THEN 'pending'
    WHEN c.outcome != 2 THEN 'not_applicable'
    WHEN c.value_cas IS NOT NULL THEN 'reference' ELSE 'not_recorded' END AS error_state,
  CASE WHEN c.value_cas IS NOT NULL THEN 'captured' ELSE 'none' END AS value_state,
  lower(hex(c.inputs_cas)) AS args_cid,
  CASE WHEN c.outcome = 1 THEN lower(hex(c.value_cas)) END AS output_cid,
  CASE WHEN c.outcome = 2 THEN lower(hex(c.value_cas)) END AS error_cid,
  lower(hex(c.inputs_cas)) AS args_cas_id,
  lower(hex(c.value_cas)) AS value_cas_id,
  __btel_ref(1, c.inputs_cas, f.argument_names,
    c.needs_announcement = 1 AND c.announced_sequence IS NULL) AS args,
  CASE WHEN c.outcome = 1 AND c.conflict = 0 THEN __btel_ref(2, c.value_cas, NULL, 0) END AS output,
  CASE WHEN c.outcome = 2 AND c.conflict = 0 THEN __btel_ref(3, c.value_cas, NULL, 0) END AS error
FROM main.call c
JOIN main.recording r ON r.rec = c.rec
LEFT JOIN main.thread t ON t.rec = c.rec AND t.thread_id = c.thread_id
LEFT JOIN main.thread root ON root.rec = t.rec AND root.thread_id = t.root_id
LEFT JOIN main.call pc ON pc.rec = c.rec AND pc.call_id = c.parent_id
LEFT JOIN main.thread pt ON pt.rec = c.rec AND pt.thread_id = c.parent_id
LEFT JOIN main.call_path p ON p.rec = c.rec AND p.call_path_id = c.call_path_id
LEFT JOIN main.function_def f ON f.rec = c.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.error_link el INDEXED BY error_link_unwound
  ON el.rec = c.rec AND el.call_id = c.call_id AND el.role = 2
LEFT JOIN main.error_raise er ON er.rec = el.rec AND er.raise_id = el.raise_id
LEFT JOIN main.epoch e ON e.rec = c.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = c.rec AND s.epoch_id = t.epoch_id",
};

pub const ERROR_CALLS: Relation = Relation {
    name: "error_calls",
    doc: "Retained calls that ended in an error, one row per call (the calls columns). Not throw occurrences: several rows can come from one propagating error, and equal error values do not mean the same throw. error_raise_id and error_occurrence_id link a row to the raise that failed it; error_occurrences counts distinct errors.",
    columns: CALL_COLUMNS,
    view: "
CREATE TEMP VIEW error_calls AS
SELECT * FROM temp.calls WHERE status = 'errored'",
};

const RAISE_KIND_DOC: &str = "throw, rethrow (a caught value raised again: bare rethrow, no-match fall-through or defer), panic_rethrow, await (an awaited future failed), await_cancelled, native_boundary (a native function or call setup failed at this BAML call), host_boundary (the engine injected a sys-op or host failure), runtime (the VM raised a panic at this instruction)";

const RAISE_SITE_DOC: &str = "resolved, raise_missing, no_pc (no bytecode frame), no_function, or why the function's map cannot place it (see call_paths.call_site_state). For boundary kinds this is the BAML call that entered native or host code, not a location inside it";

pub const ERROR_RAISES: Relation = Relation {
    name: "error_raises",
    doc: "Every observed start of unwinding (a raise), including throws caught in the same function. Each raise has its own id even when two errors have equal values. A raise that passes an error along names its occurrence only when the VM proved it through a catch landing or a failed future; otherwise origin_state says ambiguous or unresolved. An UnknownError conversion never establishes a proven link: the VM does not record where the converted value came from. When its throw carries a context kept from an earlier throw of an equal value, the raise is unresolved (source_not_recorded) and that trace is kept in inherited_trace; a conversion of a value never thrown before is a fresh raise.",
    columns: &[
        col(
            "raise_id",
            "text",
            "<recording_id>:<n>, in the same id space as threads and calls",
        ),
        col("recording_id", "text", ""),
        col("execution_id", "text", ""),
        col("thread_id", "text", "the thread that unwound"),
        col("kind", "text", RAISE_KIND_DOC),
        col(
            "occurrence_id",
            "text",
            "the occurrence's first raise (see error_occurrences): this raise when fresh, the proven origin otherwise; NULL when ambiguous or unresolved",
        ),
        col(
            "origin_state",
            "text",
            "fresh, proven, ambiguous, unresolved",
        ),
        col(
            "origin_via",
            "text",
            "rethrow (a catch landing in the same frame), await (the failed future's escaping raise), normalization (an UnknownError conversion carrying an earlier throw's context; never proven); NULL when fresh",
        ),
        col(
            "origin_candidates",
            "integer",
            "ambiguous only: how many different origins matched",
        ),
        col(
            "unresolved_reason",
            "text",
            "unresolved only: no_landing, landings_evicted, future_link_missing, future_link_evicted, no_future, source_not_recorded",
        ),
        col(
            "previous_raise_id",
            "text",
            "proven only: the one earlier raise that delivered this error here, when exactly one did",
        ),
        col(
            "function_id",
            "text",
            "the innermost BAML function when unwinding started",
        ),
        col("fqn", "text", ""),
        col(
            "call_id",
            "text",
            "that frame's retained call, when it was retained (a timing-only frame has none)",
        ),
        col(
            "pc",
            "integer",
            "live byte offset in that function's compact bytecode",
        ),
        col("site_state", "text", RAISE_SITE_DOC),
        col("site_file", "text", "as recorded; today's file may differ"),
        col("site_line", "integer", ""),
        col("site_start", "integer", ""),
        col("site_end", "integer", ""),
        col("raised_ticks", "text", "raw tick on the thread's clock"),
        col("raised_at", "text", "UTC estimate, RFC 3339"),
        col(
            "unwind_result",
            "text",
            "caught, unhandled (escaped the thread), escaped_to_native (returned to a native caller), aborted (the unwinder failed), end_not_indexed, conflicted",
        ),
        col(
            "handler_function_id",
            "text",
            "caught: the catching function",
        ),
        col("handler_fqn", "text", ""),
        col(
            "handler_site_state",
            "text",
            "caught: where the handler starts, with the same states as site_state",
        ),
        col("handler_site_file", "text", ""),
        col("handler_site_line", "integer", ""),
        col(
            "unwound_frames",
            "integer",
            "bytecode frames this raise popped, retained or not",
        ),
        col(
            "failed_calls",
            "integer",
            "retained calls this raise unwound (see error_call_links)",
        ),
        col(
            "stack_depth",
            "integer",
            "frames on the stack when it started",
        ),
        col(
            "stack_state",
            "text",
            "complete; partial: error_frames lists fewer frames than stack_depth (native frames and direct recursion are not on call paths; at most 64 are listed); path_missing: the call path naming the stack is not indexed yet",
        ),
        col(
            "inherited_trace",
            "text",
            "JSON [{function, file, line}]: the diagnostic trace the VM carried from an earlier throw, recorded only when the origin is not proven. Weaker evidence: names and lines, not raise ids",
        ),
        col("inherited_trace_state", "text", "none, complete, truncated"),
        col(
            "evidence_state",
            "text",
            "complete, end_not_indexed, raise_missing (only its end was indexed), conflicted",
        ),
    ],
    view: "
CREATE TEMP VIEW error_raises AS
SELECT __btel_pubid(r.recording_id, e.raise_id) AS raise_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  __btel_pubid(r.recording_id, e.thread_id) AS thread_id,
  CASE e.kind WHEN 1 THEN 'throw' WHEN 2 THEN 'rethrow' WHEN 3 THEN 'panic_rethrow'
    WHEN 4 THEN 'await' WHEN 5 THEN 'await_cancelled' WHEN 6 THEN 'native_boundary'
    WHEN 7 THEN 'host_boundary' WHEN 8 THEN 'runtime' END AS kind,
  __btel_pubid(r.recording_id, CASE e.origin_state WHEN 1 THEN e.raise_id
    WHEN 2 THEN e.origin_raise_id END) AS occurrence_id,
  CASE e.origin_state WHEN 1 THEN 'fresh' WHEN 2 THEN 'proven' WHEN 3 THEN 'ambiguous'
    WHEN 4 THEN 'unresolved' END AS origin_state,
  CASE e.origin_via WHEN 1 THEN 'rethrow' WHEN 2 THEN 'await' WHEN 3 THEN 'normalization' END
    AS origin_via,
  e.origin_candidates,
  CASE e.unresolved_reason WHEN 1 THEN 'no_landing' WHEN 2 THEN 'landings_evicted'
    WHEN 3 THEN 'future_link_missing' WHEN 4 THEN 'future_link_evicted' WHEN 5 THEN 'no_future'
    WHEN 6 THEN 'source_not_recorded' END AS unresolved_reason,
  __btel_pubid(r.recording_id, e.previous_raise_id) AS previous_raise_id,
  __btel_sid(r.recording_id, 'f', e.function_id) AS function_id,
  f.fqn,
  (SELECT __btel_pubid(r.recording_id, l.call_id) FROM main.error_link l
   WHERE l.rec = e.rec AND l.raise_id = e.raise_id AND l.role = 1 LIMIT 1) AS call_id,
  e.pc,
  CASE WHEN e.defined = 0 THEN 'raise_missing' ELSE e.site_state END AS site_state,
  e.site_file, e.site_line, e.site_start, e.site_end,
  CAST(e.raised_ticks AS TEXT) AS raised_ticks,
  __btel_utc(e.raised_ticks, IIF(ep.conflict = 0, ep.utc_ticks, NULL), ep.utc_unix_ns,
    ep.multiplier, ep.shift) AS raised_at,
  CASE WHEN e.end_conflict = 1 THEN 'conflicted' WHEN e.end_result IS NULL THEN 'end_not_indexed'
    ELSE CASE e.end_result WHEN 1 THEN 'caught' WHEN 2 THEN 'unhandled'
      WHEN 3 THEN 'escaped_to_native' WHEN 4 THEN 'aborted' END END AS unwind_result,
  __btel_sid(r.recording_id, 'f', e.handler_function_id) AS handler_function_id,
  hf.fqn AS handler_fqn,
  e.handler_site_state, e.handler_site_file, e.handler_site_line,
  e.unwound_frames,
  (SELECT COUNT(*) FROM main.error_link l
   WHERE l.rec = e.rec AND l.raise_id = e.raise_id AND l.role = 2) AS failed_calls,
  e.frame_count AS stack_depth,
  CASE WHEN e.frame_count IS NULL THEN NULL
    WHEN e.call_path_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM main.raise_path q
      WHERE q.rec = e.rec AND q.call_path_id = e.call_path_id AND q.built = 1)
      THEN 'path_missing'
    WHEN e.frame_count > IIF(e.call_path_id IS NULL,
      (SELECT COUNT(*) FROM main.error_frame x WHERE x.rec = e.rec AND x.raise_id = e.raise_id),
      1 + (SELECT COUNT(*) FROM main.raise_path_frame s
        WHERE s.rec = e.rec AND s.call_path_id = e.call_path_id)) THEN 'partial'
    ELSE 'complete' END AS stack_state,
  e.inherited AS inherited_trace,
  CASE WHEN e.inherited IS NULL THEN 'none'
    WHEN json_array_length(e.inherited) < e.inherited_count THEN 'truncated'
    ELSE 'complete' END AS inherited_trace_state,
  CASE WHEN e.defined = 0 THEN 'raise_missing'
    WHEN e.conflict = 1 OR e.end_conflict = 1 THEN 'conflicted'
    WHEN e.end_result IS NULL THEN 'end_not_indexed' ELSE 'complete' END AS evidence_state
FROM main.error_raise e
JOIN main.recording r ON r.rec = e.rec
LEFT JOIN main.thread t ON t.rec = e.rec AND t.thread_id = e.thread_id
LEFT JOIN main.function_def f ON f.rec = e.rec AND f.function_id = e.function_id
LEFT JOIN main.function_def hf ON hf.rec = e.rec AND hf.function_id = e.handler_function_id
LEFT JOIN main.epoch ep ON ep.rec = t.rec AND ep.epoch_id = t.epoch_id AND ep.defined = 1",
};

pub const ERROR_OCCURRENCES: Relation = Relation {
    name: "error_occurrences",
    doc: "Distinct errors: one row per fresh raise, with the raises proven to pass it along and the retained calls they failed. Equal values thrown twice are two rows. Raises whose origin is ambiguous or unresolved belong to no row; error_raises lists them.",
    columns: &[
        col("occurrence_id", "text", "the occurrence's first raise id"),
        col("recording_id", "text", ""),
        col(
            "execution_id",
            "text",
            "execution of the thread that raised it",
        ),
        col("thread_id", "text", ""),
        col("kind", "text", RAISE_KIND_DOC),
        col("function_id", "text", "where it started"),
        col("fqn", "text", ""),
        col(
            "call_id",
            "text",
            "the raising frame's retained call, if any",
        ),
        col("site_state", "text", RAISE_SITE_DOC),
        col("site_file", "text", "as recorded; today's file may differ"),
        col("site_line", "integer", ""),
        col("site_start", "integer", ""),
        col("site_end", "integer", ""),
        col("raised_at", "text", "UTC estimate, RFC 3339"),
        col(
            "raises",
            "integer",
            "this raise plus every raise proven to pass it along (rethrows and awaits; conversions are never proven)",
        ),
        col(
            "failed_calls",
            "integer",
            "retained calls those raises unwound. Timing-only calls fail too but have no ids; see call_path_stats for counts",
        ),
        col(
            "unhandled_raises",
            "integer",
            "of those raises, how many escaped their thread",
        ),
        col(
            "error_state",
            "text",
            "captured (a failed retained call recorded the value) or not_captured",
        ),
        col("error_cid", "text", "content id of that captured value"),
        value(
            "error",
            "the captured error value, read from a failed call of its first raise",
        ),
    ],
    view: "
CREATE TEMP VIEW error_occurrences AS
SELECT __btel_pubid(r.recording_id, e.raise_id) AS occurrence_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  __btel_pubid(r.recording_id, e.thread_id) AS thread_id,
  CASE e.kind WHEN 1 THEN 'throw' WHEN 2 THEN 'rethrow' WHEN 3 THEN 'panic_rethrow'
    WHEN 4 THEN 'await' WHEN 5 THEN 'await_cancelled' WHEN 6 THEN 'native_boundary'
    WHEN 7 THEN 'host_boundary' WHEN 8 THEN 'runtime' END AS kind,
  __btel_sid(r.recording_id, 'f', e.function_id) AS function_id,
  f.fqn,
  (SELECT __btel_pubid(r.recording_id, l.call_id) FROM main.error_link l
   WHERE l.rec = e.rec AND l.raise_id = e.raise_id AND l.role = 1 LIMIT 1) AS call_id,
  e.site_state, e.site_file, e.site_line, e.site_start, e.site_end,
  __btel_utc(e.raised_ticks, IIF(ep.conflict = 0, ep.utc_ticks, NULL), ep.utc_unix_ns,
    ep.multiplier, ep.shift) AS raised_at,
  1 + (SELECT COUNT(*) FROM main.error_raise x INDEXED BY error_raise_by_origin
    WHERE x.rec = e.rec AND x.origin_raise_id = e.raise_id AND x.origin_state = 2) AS raises,
  (SELECT COUNT(*) FROM main.error_link l
   WHERE l.rec = e.rec AND l.role = 2 AND l.raise_id IN (
     SELECT e.raise_id UNION ALL
     SELECT x.raise_id FROM main.error_raise x INDEXED BY error_raise_by_origin
     WHERE x.rec = e.rec AND x.origin_raise_id = e.raise_id AND x.origin_state = 2))
    AS failed_calls,
  IIF(e.end_result = 2, 1, 0) + (SELECT COUNT(*) FROM main.error_raise x
    INDEXED BY error_raise_by_origin
    WHERE x.rec = e.rec AND x.origin_raise_id = e.raise_id AND x.origin_state = 2
      AND x.end_result = 2) AS unhandled_raises,
  IIF(vc.value_cas IS NULL, 'not_captured', 'captured') AS error_state,
  lower(hex(vc.value_cas)) AS error_cid,
  __btel_ref(3, vc.value_cas, NULL, 0) AS error
FROM main.error_raise e
JOIN main.recording r ON r.rec = e.rec
LEFT JOIN main.thread t ON t.rec = e.rec AND t.thread_id = e.thread_id
LEFT JOIN main.function_def f ON f.rec = e.rec AND f.function_id = e.function_id
LEFT JOIN main.epoch ep ON ep.rec = t.rec AND ep.epoch_id = t.epoch_id AND ep.defined = 1
LEFT JOIN main.call vc ON vc.rec = e.rec AND vc.call_id = (
  SELECT l.call_id FROM main.error_link l
  JOIN main.call c ON c.rec = l.rec AND c.call_id = l.call_id
  WHERE l.rec = e.rec AND l.raise_id = e.raise_id AND l.role = 2 AND c.outcome = 2
    AND c.conflict = 0 AND c.value_cas IS NOT NULL
  ORDER BY c.entered_ticks LIMIT 1)
WHERE e.defined = 1 AND e.origin_state = 1",
};

pub const ERROR_FRAMES: Relation = Relation {
    name: "error_frames",
    doc: "The stack of each raise when unwinding started, innermost first (position 0), with sites resolved in the recorded source maps. At most 64 frames per raise. Usually derived from the raising frame's call path, which omits native frames and lists direct recursion once; error_raises.stack_state says whether frames are missing.",
    columns: &[
        col("raise_id", "text", ""),
        col("recording_id", "text", ""),
        col("position", "integer", "0 is the raising frame"),
        col("function_id", "text", ""),
        col("fqn", "text", ""),
        col(
            "pc",
            "integer",
            "the live PC at position 0; the call site in outer frames; NULL for native frames",
        ),
        col("native", "integer", "1 for a native function frame"),
        col(
            "site_state",
            "text",
            "as error_raises.site_state; native frames have no_pc",
        ),
        col("site_file", "text", "as recorded"),
        col("site_line", "integer", ""),
        col("site_start", "integer", ""),
        col("site_end", "integer", ""),
    ],
    view: "
CREATE TEMP VIEW error_frames AS
SELECT __btel_pubid(r.recording_id, x.raise_id) AS raise_id,
  lower(hex(r.recording_id)) AS recording_id,
  x.position,
  __btel_sid(r.recording_id, 'f', x.function_id) AS function_id,
  f.fqn, x.pc, x.native,
  x.site_state, x.site_file, x.site_line, x.site_start, x.site_end
FROM (
  SELECT rec, raise_id, position, function_id, pc, native,
    site_state, site_file, site_line, site_start, site_end
  FROM main.error_frame
  UNION ALL
  SELECT e.rec, e.raise_id, 0, e.function_id, e.pc, 0,
    e.site_state, e.site_file, e.site_line, e.site_start, e.site_end
  FROM main.error_raise e WHERE e.call_path_id IS NOT NULL
  UNION ALL
  SELECT e.rec, e.raise_id, s.position, p.caller_function_id, p.caller_pc, 0,
    p.site_state, p.site_file, p.site_line, p.site_start, p.site_end
  -- Raises drive: raise_path_frame and call_path are keyed lookups, and
  -- error_raise has no call-path index to cost every import.
  FROM main.error_raise e
  CROSS JOIN main.raise_path_frame s ON s.rec = e.rec AND s.call_path_id = e.call_path_id
  JOIN main.call_path p ON p.rec = s.rec AND p.call_path_id = s.frame_path_id
  WHERE e.call_path_id IS NOT NULL
) x
CROSS JOIN main.recording r ON r.rec = x.rec
LEFT JOIN main.function_def f ON f.rec = x.rec AND f.function_id = x.function_id",
};

pub const ERROR_CALL_LINKS: Relation = Relation {
    name: "error_call_links",
    doc: "Retained calls each raise names: the call whose frame raised it, and every call it unwound. A call is unwound by at most one raise. Timing-only calls have no ids and do not appear.",
    columns: &[
        col("raise_id", "text", ""),
        col(
            "occurrence_id",
            "text",
            "NULL when the raise's origin is ambiguous or unresolved",
        ),
        col("recording_id", "text", ""),
        col("call_id", "text", ""),
        col(
            "role",
            "text",
            "raise_frame (it raised here; it may have caught its own error), unwound (it failed while this raise unwound it)",
        ),
        col("fqn", "text", "the call's function"),
        col(
            "call_status",
            "text",
            "ok, errored, cancelled, incomplete, as in calls.status",
        ),
    ],
    view: "
CREATE TEMP VIEW error_call_links AS
SELECT __btel_pubid(r.recording_id, l.raise_id) AS raise_id,
  __btel_pubid(r.recording_id, CASE e.origin_state WHEN 1 THEN e.raise_id
    WHEN 2 THEN e.origin_raise_id END) AS occurrence_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, l.call_id) AS call_id,
  CASE l.role WHEN 1 THEN 'raise_frame' WHEN 2 THEN 'unwound' END AS role,
  f.fqn,
  CASE WHEN c.conflict > 0 THEN 'conflicted'
    ELSE CASE c.outcome WHEN 1 THEN 'ok' WHEN 2 THEN 'errored' WHEN 3 THEN 'cancelled'
      ELSE 'incomplete' END END AS call_status
FROM main.error_link l
JOIN main.recording r ON r.rec = l.rec
LEFT JOIN main.error_raise e ON e.rec = l.rec AND e.raise_id = l.raise_id
LEFT JOIN main.call c ON c.rec = l.rec AND c.call_id = l.call_id
LEFT JOIN main.call_path p ON p.rec = c.rec AND p.call_path_id = c.call_path_id
LEFT JOIN main.function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id",
};

pub const FUNCTION_DEFINITIONS: Relation = Relation {
    name: "function_definitions",
    doc: "Functions the recording referenced, with the metadata recorded for them (not every compiled function). Names come from the recording, never from today's source.",
    columns: &[
        col("function_id", "text", "<recording_id>:f<n>"),
        col("recording_id", "text", ""),
        col(
            "local_function_id",
            "text",
            "the runtime function id; only meaningful within its recording",
        ),
        col(
            "definition_state",
            "text",
            "resolved, unavailable (the producer could not look it up), unresolved (referenced, no definition indexed), conflicted",
        ),
        col("fqn", "text", "fully qualified name"),
        col("display_name", "text", ""),
        col(
            "definition_key",
            "text",
            "stable logical identity, when recorded",
        ),
        col("kind", "text", "bytecode, sysop, native, native_unresolved"),
        col("kind_detail", "text", "the sysop name for sysop functions"),
        col(
            "origin",
            "text",
            "user, companion, internal, builtin, auto_derive",
        ),
        col("source_file", "text", "file of the definition"),
        col(
            "source_file_id",
            "integer",
            "compiler-local file number; not unique across recordings",
        ),
        col(
            "source_start",
            "integer",
            "definition span start offset (not a call site)",
        ),
        col("source_end", "integer", "definition span end offset"),
        col("package", "text", ""),
        col("namespace", "text", "namespace components joined with '.'"),
        col(
            "namespace_components",
            "text",
            "the components as a JSON array",
        ),
        col("owner_type_key", "text", "owning type of a method"),
        col(
            "parent_function_key",
            "text",
            "enclosing function of a lambda",
        ),
        col("lambda_path", "text", ""),
        col(
            "argument_layout_state",
            "text",
            "known (see function_parameters), unknown, conflicted",
        ),
        col(
            "parameter_count",
            "integer",
            "slots in the recorded layout; 0 is a known zero-parameter function, NULL unknown",
        ),
    ],
    view: "
CREATE TEMP VIEW function_definitions AS
SELECT __btel_sid(r.recording_id, 'f', f.function_id) AS function_id,
  lower(hex(r.recording_id)) AS recording_id,
  __btel_u64(f.function_id) AS local_function_id,
  CASE WHEN f.conflict = 1 THEN 'conflicted' WHEN f.state = 2 THEN 'resolved'
    WHEN f.state = 1 THEN 'unavailable' ELSE 'unresolved' END AS definition_state,
  f.fqn, f.display_name, f.definition_key, f.kind, f.kind_detail, f.origin, f.source_file,
  f.source_file_id, f.source_start, f.source_end, f.package,
  f.namespace, f.namespace_json AS namespace_components,
  f.owner_type_key, f.parent_function_key, f.lambda_path,
  CASE WHEN f.conflict = 1 THEN 'conflicted' WHEN f.parameter_count IS NOT NULL THEN 'known'
    ELSE 'unknown' END AS argument_layout_state,
  f.parameter_count
FROM main.function_def f
JOIN main.recording r ON r.rec = f.rec",
};

pub const FUNCTION_PARAMETERS: Relation = Relation {
    name: "function_parameters",
    doc: "Recorded parameter slots, for functions whose layout was recorded. These are the names args['name'] resolves.",
    columns: &[
        col("recording_id", "text", ""),
        col("function_id", "text", ""),
        col("fqn", "text", ""),
        col("position", "integer", "zero-based slot: args[position]"),
        col("name", "text", "NULL for an unnamed slot"),
        col("is_receiver", "integer", "1 for a method's self slot"),
    ],
    view: "
CREATE TEMP VIEW function_parameters AS
SELECT lower(hex(r.recording_id)) AS recording_id,
  __btel_sid(r.recording_id, 'f', a.function_id) AS function_id,
  f.fqn, a.position, a.name, a.receiver AS is_receiver
FROM main.function_param a
JOIN main.recording r ON r.rec = a.rec
JOIN main.function_def f ON f.rec = a.rec AND f.function_id = a.function_id",
};

pub const CALL_PATHS: Relation = Relation {
    name: "call_paths",
    doc: "Calling contexts: a function called at one bytecode site below one parent context. A direct recursive call reuses its caller's path (see call_path_nodes), so this is not a tree of every VM frame.",
    columns: &[
        col("call_path_id", "text", "<recording_id>:p<n>"),
        col("recording_id", "text", ""),
        col("local_call_path_id", "integer", "the recorded path id"),
        col("execution_id", "text", "execution of the defining thread"),
        col(
            "definition_state",
            "text",
            "resolved, unresolved (referenced, no definition indexed), conflicted",
        ),
        col(
            "structure_state",
            "text",
            "resolved (depth and execution known), unresolved, conflicted",
        ),
        col(
            "thread_id",
            "text",
            "thread that first used this context; other threads never share it",
        ),
        col("clock_epoch_id", "text", ""),
        col(
            "parent_call_path_id",
            "text",
            "enclosing context; NULL for a thread's top-level call",
        ),
        col(
            "depth",
            "integer",
            "0 for top-level contexts; spawned threads continue their spawn edge's depth",
        ),
        col(
            "caller_function_id",
            "text",
            "the visible calling function, if recorded",
        ),
        col("caller_fqn", "text", ""),
        col(
            "caller_pc",
            "integer",
            "byte offset of the call or spawn instruction in the caller's compact bytecode",
        ),
        col(
            "call_site_state",
            "text",
            "resolved; no_caller (an entry call from the host or a thread's first call); path_unresolved; function_unresolved or function_unavailable (the caller's definition is not indexed or could not be looked up); no_source_map (an older recording); invalid_source_map; no_source_file; foreign_file; unmapped_pc; pc_out_of_range; sentinel_pc",
        ),
        col(
            "call_site_file",
            "text",
            "file of the call or spawn expression, as recorded; today's file may have changed",
        ),
        col(
            "call_site_line",
            "integer",
            "one-based line. Every non-recursive invocation of this context entered here; a direct recursive re-entry reuses the context without a site of its own",
        ),
        col(
            "call_site_start",
            "integer",
            "byte offset where the expression starts",
        ),
        col("call_site_end", "integer", "byte offset where it ends"),
        col("function_id", "text", "the called function"),
        col("fqn", "text", ""),
        col("definition_key", "text", ""),
        col("kind", "text", ""),
        col("origin", "text", ""),
        col("recorded_edge", "text", "synchronous or spawn"),
        col(
            "edge_kind",
            "text",
            "root (an execution's entry), call, spawn",
        ),
    ],
    view: "
CREATE TEMP VIEW call_paths AS
SELECT __btel_sid(r.recording_id, 'p', p.call_path_id) AS call_path_id,
  lower(hex(r.recording_id)) AS recording_id,
  p.call_path_id AS local_call_path_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  CASE WHEN p.defined = 0 THEN 'unresolved' WHEN p.conflict = 1 THEN 'conflicted'
    ELSE 'resolved' END AS definition_state,
  CASE WHEN p.conflict = 1 THEN 'conflicted'
    WHEN p.depth IS NOT NULL AND t.root_id IS NOT NULL THEN 'resolved'
    ELSE 'unresolved' END AS structure_state,
  __btel_pubid(r.recording_id, p.thread_id) AS thread_id,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  __btel_sid(r.recording_id, 'p', NULLIF(p.parent_call_path_id, 0)) AS parent_call_path_id,
  p.depth,
  __btel_sid(r.recording_id, 'f', p.caller_function_id) AS caller_function_id,
  cf.fqn AS caller_fqn,
  p.caller_pc,
  COALESCE(p.site_state, 'path_unresolved') AS call_site_state,
  p.site_file AS call_site_file, p.site_line AS call_site_line,
  p.site_start AS call_site_start, p.site_end AS call_site_end,
  __btel_sid(r.recording_id, 'f', p.callee_function_id) AS function_id,
  f.fqn, f.definition_key, f.kind, f.origin,
  CASE p.edge WHEN 1 THEN 'synchronous' WHEN 2 THEN 'spawn' END AS recorded_edge,
  CASE WHEN p.edge = 2 THEN 'spawn'
    WHEN p.edge = 1 AND p.parent_call_path_id = 0 AND t.defined = 1 AND t.parent_id IS NULL
      THEN 'root'
    WHEN p.edge = 1 THEN 'call' END AS edge_kind
FROM main.call_path p
JOIN main.recording r ON r.rec = p.rec
LEFT JOIN main.thread t ON t.rec = p.rec AND t.thread_id = p.thread_id
LEFT JOIN main.function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.function_def cf ON cf.rec = p.rec AND cf.function_id = p.caller_function_id",
};

pub const CALL_PATH_NODES: Relation = Relation {
    name: "call_path_nodes",
    doc: "Reduced aggregate totals exactly as recorded: one row per calling context and reentry bit. Reentry rows are direct recursive re-entries; their durations overlap the outer invocation's.",
    columns: &[
        col("recording_id", "text", ""),
        col("call_path_id", "text", ""),
        col("execution_id", "text", ""),
        col("fqn", "text", ""),
        col(
            "reentry",
            "integer",
            "0 outermost invocations, 1 direct recursive re-entries",
        ),
        col(
            "completed_calls",
            "integer",
            "completions aggregated; NULL if it no longer fits",
        ),
        col("completed_calls_exact", "text", "exact decimal count"),
        col(
            "duration_sum_ns",
            "integer",
            "sum of these invocations' durations",
        ),
        col("duration_ticks_exact", "text", "exact decimal tick sum"),
        col("self_await_ns", "integer", "sum of their own await time"),
        col("self_await_ticks_exact", "text", "exact decimal tick sum"),
        col("clock_epoch_id", "text", ""),
        col(
            "timing_state",
            "text",
            "valid, unknown_clock, conflicted, invalidated_*, overflow",
        ),
        col(
            "aggregate_state",
            "text",
            "observed_prefix, overflow, unresolved (the path is not defined yet)",
        ),
        OK_CALLS,
        ERRORED_CALLS,
        CANCELLED_CALLS,
        OUTCOME_STATE,
    ],
    view: "
CREATE TEMP VIEW call_path_nodes AS
SELECT lower(hex(r.recording_id)) AS recording_id,
  __btel_sid(r.recording_id, 'p', a.node >> 1) AS call_path_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  f.fqn,
  a.node & 1 AS reentry,
  a.call_count AS completed_calls,
  a.count_exact AS completed_calls_exact,
  __btel_total_ns(a.duration_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration_sum_ns,
  a.duration_exact AS duration_ticks_exact,
  __btel_total_ns(a.self_await_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS self_await_ns,
  a.self_await_exact AS self_await_ticks_exact,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  __btel_total_timing(a.duration_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS timing_state,
  CASE WHEN p.defined IS NOT 1 THEN 'unresolved'
    WHEN a.call_count IS NULL OR a.duration_ticks IS NULL OR a.self_await_ticks IS NULL
      THEN 'overflow'
    ELSE 'observed_prefix' END AS aggregate_state,
  a.ok_calls, a.errored_calls, a.cancelled_calls,
  __btel_outcome_state(a.outcome_evidence, a.call_count, a.errored_calls, a.cancelled_calls) AS outcome_state
FROM main.aggregate a
JOIN main.recording r ON r.rec = a.rec
LEFT JOIN main.call_path p ON p.rec = a.rec AND p.call_path_id = (a.node >> 1)
LEFT JOIN main.thread t ON t.rec = p.rec AND t.thread_id = p.thread_id
LEFT JOIN main.function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = t.rec AND s.epoch_id = t.epoch_id",
};

pub const CALL_PATH_STATS: Relation = Relation {
    name: "call_path_stats",
    doc: "Population timing per calling context, recursion-aware. Counts are completed invocations (not starts) over every recorded invocation, not only retained calls.",
    columns: &[
        col("recording_id", "text", ""),
        col("execution_id", "text", ""),
        col("call_path_id", "text", ""),
        col("parent_call_path_id", "text", ""),
        col("depth", "integer", ""),
        col("function_id", "text", ""),
        col("fqn", "text", ""),
        col("definition_key", "text", ""),
        col("kind", "text", ""),
        col("origin", "text", ""),
        col("edge_kind", "text", "root, call, spawn"),
        col("clock_epoch_id", "text", ""),
        col(
            "normal_completed_calls",
            "integer",
            "completed outermost invocations",
        ),
        col(
            "reentry_completed_calls",
            "integer",
            "completed direct recursive re-entries",
        ),
        col(
            "completed_calls",
            "integer",
            "all completed invocations of this context (both)",
        ),
        col(
            "invocation_duration_sum_ns",
            "integer",
            "sum of every completed invocation's duration, recursive ones included (overlapping time counts twice); use for average duration",
        ),
        col(
            "inclusive_ns",
            "integer",
            "time inside this context without recursive double counting: the outermost invocations' durations",
        ),
        col(
            "reentry_duration_ns",
            "integer",
            "sum of recursive re-entries' durations (already inside inclusive_ns)",
        ),
        col(
            "direct_child_ns",
            "integer",
            "inclusive time of direct synchronous child contexts (spawned work excluded)",
        ),
        col(
            "await_ns",
            "integer",
            "time this context's own frames spent awaiting, recursive ones included",
        ),
        col(
            "self_ns",
            "integer",
            "inclusive minus children minus await; NULL when self_time_state says it is unsupported. Not CPU time",
        ),
        col(
            "timing_state",
            "text",
            "valid, unknown_clock, conflicted, invalidated_*, overflow",
        ),
        col(
            "structure_state",
            "text",
            "resolved, unresolved, conflicted",
        ),
        col(
            "aggregate_state",
            "text",
            "observed_prefix, none_observed (defined, no completion yet), overflow, unresolved",
        ),
        col(
            "self_time_state",
            "text",
            "valid (thread finished), provisional (thread still unfinished in the prefix), incomplete (outer invocation not completed yet), underflow (evidence inconsistent), unresolved, overflow, or a clock state",
        ),
        OK_CALLS,
        ERRORED_CALLS,
        CANCELLED_CALLS,
        OUTCOME_STATE,
    ],
    view: "
CREATE TEMP VIEW call_path_stats AS
SELECT lower(hex(r.recording_id)) AS recording_id,
  __btel_pubid(r.recording_id, t.root_id) AS execution_id,
  __btel_sid(r.recording_id, 'p', p.call_path_id) AS call_path_id,
  __btel_sid(r.recording_id, 'p', NULLIF(p.parent_call_path_id, 0)) AS parent_call_path_id,
  p.depth,
  __btel_sid(r.recording_id, 'f', p.callee_function_id) AS function_id,
  f.fqn, f.definition_key, f.kind, f.origin,
  CASE WHEN p.edge = 2 THEN 'spawn'
    WHEN p.edge = 1 AND p.parent_call_path_id = 0 AND t.defined = 1 AND t.parent_id IS NULL
      THEN 'root'
    WHEN p.edge = 1 THEN 'call' END AS edge_kind,
  __btel_sid(r.recording_id, 'c', t.epoch_id) AS clock_epoch_id,
  IIF(n.rec IS NULL, 0, n.call_count) AS normal_completed_calls,
  IIF(x.rec IS NULL, 0, x.call_count) AS reentry_completed_calls,
  __btel_add(IIF(n.rec IS NULL, 0, n.call_count), IIF(x.rec IS NULL, 0, x.call_count))
    AS completed_calls,
  __btel_total_ns(__btel_add(IIF(n.rec IS NULL, 0, n.duration_ticks),
      IIF(x.rec IS NULL, 0, x.duration_ticks)),
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict))
    AS invocation_duration_sum_ns,
  IIF(n.rec IS NULL, NULL, __btel_total_ns(n.duration_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict))) AS inclusive_ns,
  __btel_total_ns(IIF(x.rec IS NULL, 0, x.duration_ticks), e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS reentry_duration_ns,
  __btel_total_ns(
    p.direct_child_ticks,
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS direct_child_ns,
  __btel_total_ns(__btel_add(IIF(n.rec IS NULL, 0, n.self_await_ticks),
      IIF(x.rec IS NULL, 0, x.self_await_ticks)),
    e.multiplier, e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS await_ns,
  __btel_self(0, p.defined, n.rec IS NOT NULL, n.duration_ticks, n.self_await_ticks,
    x.rec IS NOT NULL, x.self_await_ticks,
    p.direct_child_ticks,
    t.outcome IS NOT NULL, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS self_ns,
  __btel_total_timing(IIF(n.rec IS NULL, 0, n.duration_ticks), e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS timing_state,
  CASE WHEN p.conflict = 1 THEN 'conflicted'
    WHEN p.depth IS NOT NULL AND t.root_id IS NOT NULL THEN 'resolved'
    ELSE 'unresolved' END AS structure_state,
  CASE WHEN p.defined = 0 THEN 'unresolved'
    WHEN n.rec IS NULL AND x.rec IS NULL THEN 'none_observed'
    WHEN n.call_count IS NULL OR n.duration_ticks IS NULL OR n.self_await_ticks IS NULL
      OR x.call_count IS NULL AND x.rec IS NOT NULL
      OR x.duration_ticks IS NULL AND x.rec IS NOT NULL THEN 'overflow'
    ELSE 'observed_prefix' END AS aggregate_state,
  __btel_self(1, p.defined, n.rec IS NOT NULL, n.duration_ticks, n.self_await_ticks,
    x.rec IS NOT NULL, x.self_await_ticks,
    p.direct_child_ticks,
    t.outcome IS NOT NULL, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS self_time_state,
  __btel_add(IIF(n.rec IS NULL, 0, n.ok_calls), IIF(x.rec IS NULL, 0, x.ok_calls)) AS ok_calls,
  __btel_add(IIF(n.rec IS NULL, 0, n.errored_calls), IIF(x.rec IS NULL, 0, x.errored_calls)) AS errored_calls,
  __btel_add(IIF(n.rec IS NULL, 0, n.cancelled_calls), IIF(x.rec IS NULL, 0, x.cancelled_calls)) AS cancelled_calls,
  __btel_outcome_state(COALESCE(n.outcome_evidence, 0) | COALESCE(x.outcome_evidence, 0),
    __btel_add(IIF(n.rec IS NULL, 0, n.call_count), IIF(x.rec IS NULL, 0, x.call_count)),
    __btel_add(IIF(n.rec IS NULL, 0, n.errored_calls), IIF(x.rec IS NULL, 0, x.errored_calls)),
    __btel_add(IIF(n.rec IS NULL, 0, n.cancelled_calls), IIF(x.rec IS NULL, 0, x.cancelled_calls))) AS outcome_state
FROM main.call_path p
JOIN main.recording r ON r.rec = p.rec
LEFT JOIN main.thread t ON t.rec = p.rec AND t.thread_id = p.thread_id
LEFT JOIN main.function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.aggregate n ON n.rec = p.rec AND n.node = p.call_path_id * 2
LEFT JOIN main.aggregate x ON x.rec = p.rec AND x.node = p.call_path_id * 2 + 1
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = t.rec AND s.epoch_id = t.epoch_id",
};

pub const HOT_CALL_PATHS: Relation = Relation {
    name: "hot_call_paths",
    doc: "call_path_stats rows with a supported self time. Order it yourself, e.g. ORDER BY self_ns DESC.",
    columns: &[
        col("recording_id", "text", ""),
        col("execution_id", "text", ""),
        col("call_path_id", "text", ""),
        col("function_id", "text", ""),
        col("fqn", "text", ""),
        col("depth", "integer", ""),
        col("self_ns", "integer", ""),
        col("inclusive_ns", "integer", ""),
        col("completed_calls", "integer", ""),
        col("timing_state", "text", ""),
        col("self_time_state", "text", "valid or provisional"),
    ],
    view: "
CREATE TEMP VIEW hot_call_paths AS
SELECT recording_id, execution_id, call_path_id, function_id, fqn, depth, self_ns, inclusive_ns,
  completed_calls, timing_state, self_time_state
FROM temp.call_path_stats WHERE self_ns IS NOT NULL",
};

pub const FUNCTION_STATS: Relation = Relation {
    name: "function_stats",
    doc: "call_path_stats summed per execution and function. Counts cover every recorded completed invocation, not only retained calls. Group across executions yourself.",
    columns: &[
        col(
            "execution_id",
            "text",
            "NULL when the calling thread's execution is unresolved",
        ),
        col("recording_id", "text", ""),
        col("function_id", "text", ""),
        col("fqn", "text", "NULL when no function metadata was recorded"),
        col(
            "completed_calls",
            "integer",
            "completed invocations, recursive ones included; NULL on overflow",
        ),
        col("call_count", "integer", "alias of completed_calls"),
        col(
            "invocation_duration_sum_ns",
            "integer",
            "sum of invocation durations (recursive time overlaps); divide by completed_calls for an average",
        ),
        col(
            "total_duration_ns",
            "integer",
            "alias of invocation_duration_sum_ns",
        ),
        col("await_ns", "integer", "await time inside the function"),
        col("total_self_await_ns", "integer", "alias of await_ns"),
        col(
            "self_ns",
            "integer",
            "sum of its contexts' self time; NULL if any context's self time is unsupported",
        ),
        col(
            "timing_state",
            "text",
            "valid, unknown_clock, conflicted, invalidated_*, overflow, mixed_clock",
        ),
        col(
            "self_time_state",
            "text",
            "valid, provisional, or the first reason a context's self time is unsupported",
        ),
        OK_CALLS,
        ERRORED_CALLS,
        CANCELLED_CALLS,
        OUTCOME_STATE,
    ],
    view: "
CREATE TEMP VIEW function_stats AS
SELECT execution_id, recording_id, function_id, MIN(fqn) AS fqn,
  __btel_sum(completed_calls) AS completed_calls,
  __btel_sum(completed_calls) AS call_count,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) <= 1 THEN __btel_sum(invocation_duration_sum_ns) END
    AS invocation_duration_sum_ns,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) <= 1 THEN __btel_sum(invocation_duration_sum_ns) END
    AS total_duration_ns,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) <= 1 THEN __btel_sum(await_ns) END AS await_ns,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) <= 1 THEN __btel_sum(await_ns) END
    AS total_self_await_ns,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) <= 1 THEN __btel_sum(self_ns) END AS self_ns,
  CASE WHEN COUNT(DISTINCT clock_epoch_id) > 1 THEN 'mixed_clock'
    ELSE COALESCE(MIN(NULLIF(timing_state, 'valid')), 'valid') END AS timing_state,
  CASE WHEN COUNT(self_ns) < COUNT(*) THEN MIN(IIF(self_ns IS NULL, self_time_state, NULL))
    WHEN MAX(self_time_state = 'provisional') = 1 THEN 'provisional'
    ELSE 'valid' END AS self_time_state,
  __btel_sum(ok_calls) AS ok_calls,
  __btel_sum(errored_calls) AS errored_calls,
  __btel_sum(cancelled_calls) AS cancelled_calls,
  __btel_outcome_state(__btel_outcome_evidence(outcome_state), __btel_sum(completed_calls),
    __btel_sum(errored_calls), __btel_sum(cancelled_calls)) AS outcome_state
FROM temp.call_path_stats
GROUP BY recording_id, execution_id, function_id",
};

pub const CLOCKS: Relation = Relation {
    name: "clocks",
    doc: "Clock epochs: how ticks convert to time, and what the recording observed about their validity. Timing columns elsewhere are NULL when their clock is not valid here.",
    columns: &[
        col("recording_id", "text", ""),
        col("clock_epoch_id", "text", "<recording_id>:c<n>"),
        col("domain_id", "text", "tick scale identity"),
        col(
            "definition_state",
            "text",
            "resolved, unresolved (referenced, no definition indexed), conflicted",
        ),
        col(
            "source",
            "text",
            "os_monotonic, windows_qpc, x86_tsc, arm_system_counter, mock",
        ),
        col("reference_tick", "text", ""),
        col("reference_monotonic_ns", "text", ""),
        col("multiplier", "text", "ns = (ticks * multiplier) >> shift"),
        col("shift", "integer", ""),
        col("origin_uncertainty_ns", "text", ""),
        col("rate_error_ppb", "text", ""),
        col(
            "calibration_status",
            "text",
            "not_required, converged, deadline_reached",
        ),
        col("calibration_samples", "text", ""),
        col("calibration_elapsed_ns", "text", ""),
        col("calibration_mean_residual_ns", "real", ""),
        col("calibration_mean_error_ns", "real", ""),
        col(
            "fallback_reason",
            "text",
            "none, requested, unsupported, calibration, scale_validation, discontinuity",
        ),
        col("utc_anchor_ticks", "text", ""),
        col(
            "utc_anchor_unix_ns",
            "text",
            "exact signed Unix nanoseconds of the anchor",
        ),
        col("utc_anchor_at", "text", "the anchor, RFC 3339"),
        col("utc_uncertainty_ns", "text", ""),
        col(
            "observed_status",
            "text",
            "most severe validity observed: valid, restored, discontinuity, uncertain, mode_changed",
        ),
        col(
            "is_final",
            "integer",
            "1 once the producer saw every thread of the run finish: the status can no longer change. Older recordings and runs still attached at the end stay 0",
        ),
        col(
            "timing_state",
            "text",
            "valid (final when is_final is 1, else may still be invalidated), unknown_clock, conflicted, invalidated_*",
        ),
    ],
    view: "
CREATE TEMP VIEW clocks AS
SELECT lower(hex(r.recording_id)) AS recording_id,
  __btel_sid(r.recording_id, 'c', e.epoch_id) AS clock_epoch_id,
  __btel_u64(e.domain_id) AS domain_id,
  CASE WHEN e.defined = 0 THEN 'unresolved' WHEN e.conflict = 1 THEN 'conflicted'
    ELSE 'resolved' END AS definition_state,
  __btel_epoch_field(e.definition, 'source') AS source,
  __btel_epoch_field(e.definition, 'reference_tick') AS reference_tick,
  __btel_epoch_field(e.definition, 'reference_monotonic_ns') AS reference_monotonic_ns,
  __btel_epoch_field(e.definition, 'multiplier') AS multiplier,
  __btel_epoch_field(e.definition, 'shift') AS shift,
  __btel_epoch_field(e.definition, 'origin_uncertainty_ns') AS origin_uncertainty_ns,
  __btel_epoch_field(e.definition, 'rate_error_ppb') AS rate_error_ppb,
  __btel_epoch_field(e.definition, 'calibration_status') AS calibration_status,
  __btel_epoch_field(e.definition, 'calibration_samples') AS calibration_samples,
  __btel_epoch_field(e.definition, 'calibration_elapsed_ns') AS calibration_elapsed_ns,
  __btel_epoch_field(e.definition, 'calibration_mean_residual_ns')
    AS calibration_mean_residual_ns,
  __btel_epoch_field(e.definition, 'calibration_mean_error_ns') AS calibration_mean_error_ns,
  __btel_epoch_field(e.definition, 'fallback_reason') AS fallback_reason,
  __btel_epoch_field(e.definition, 'utc_anchor_ticks') AS utc_anchor_ticks,
  __btel_epoch_field(e.definition, 'utc_anchor_unix_ns') AS utc_anchor_unix_ns,
  __btel_epoch_field(e.definition, 'utc_anchor_at') AS utc_anchor_at,
  __btel_epoch_field(e.definition, 'utc_uncertainty_ns') AS utc_uncertainty_ns,
  CASE s.status WHEN 1 THEN 'valid' WHEN 2 THEN 'restored' WHEN 3 THEN 'discontinuity'
    WHEN 4 THEN 'uncertain' WHEN 5 THEN 'mode_changed' END AS observed_status,
  COALESCE(s.final, 0) AS is_final,
  CASE WHEN e.defined = 0 OR s.status IS NULL THEN 'unknown_clock'
    WHEN e.conflict = 1 THEN 'conflicted'
    WHEN s.status = 1 THEN 'valid'
    ELSE 'invalidated_' || CASE s.status WHEN 2 THEN 'restored' WHEN 3 THEN 'discontinuity'
      WHEN 4 THEN 'uncertain' ELSE 'mode_changed' END END AS timing_state
FROM main.epoch e
JOIN main.recording r ON r.rec = e.rec
LEFT JOIN main.epoch_state s ON s.rec = e.rec AND s.epoch_id = e.epoch_id",
};

pub const ISSUES: Relation = Relation {
    name: "issues",
    doc: "Evidence problems the reader found: conflicts, gaps, invalid files, unresolved references, invalidated clocks. Separate from execution outcomes. No rows is not proof that the producer lost nothing.",
    columns: &[
        col("recording_id", "text", ""),
        col(
            "sequence",
            "integer",
            "file that produced the issue, when known",
        ),
        col("code", "text", "stable diagnostic code"),
        col(
            "severity",
            "text",
            "error (evidence stops indexing), warning (some answers are unavailable), info",
        ),
        col(
            "subject_kind",
            "text",
            "recording, file, thread, call, call_path, function, clock",
        ),
        col(
            "subject",
            "text",
            "affected identity, in the same form as the other relations' ids",
        ),
        col("message", "text", ""),
    ],
    view: "
CREATE TEMP VIEW issues AS
SELECT lower(hex(r.recording_id)) AS recording_id, i.sequence, i.code,
  'warning' AS severity,
  CASE i.code
    WHEN 'thread_completion_conflict' THEN 'thread'
    WHEN 'thread_definition_conflict' THEN 'thread'
    WHEN 'call_evidence_conflict' THEN 'call'
    WHEN 'call_path_conflict' THEN 'call_path'
    WHEN 'function_metadata_conflict' THEN 'function'
    WHEN 'clock_epoch_conflict' THEN 'clock'
    WHEN 'aggregate_outcome_invalid' THEN 'call_path_node'
    ELSE 'file' END AS subject_kind,
  CASE WHEN i.subject IS NULL THEN NULL
    ELSE lower(hex(r.recording_id)) || ':' || i.subject END AS subject,
  i.detail AS message
FROM main.issue i JOIN main.recording r ON r.rec = i.rec
UNION ALL
SELECT lower(hex(r.recording_id)), r.blocked_sequence,
  CASE r.blocked_reason WHEN 'invalid' THEN 'invalid_file' ELSE 'sequence_gap' END, 'error',
  'file', NULL,
  CASE r.blocked_reason
    WHEN 'invalid' THEN 'file ' || r.blocked_sequence || ' is invalid and stops indexing: '
      || COALESCE((SELECT j.reason FROM main.rejected j
                   WHERE j.rec = r.rec AND j.sequence = r.blocked_sequence), '')
    ELSE 'file ' || r.blocked_sequence || ' is missing; files ' || r.blocked_sequence
      || ' to ' || r.observed_sequence || ' are not indexed' END
FROM main.recording r WHERE r.blocked_sequence IS NOT NULL
UNION ALL
SELECT lower(hex(r.recording_id)), r.terminal_sequence, 'files_after_end', 'warning',
  'recording', NULL, r.files_after_end || ' file(s) after the end marker are not indexed'
FROM main.recording r WHERE r.files_after_end > 0
UNION ALL
SELECT lower(hex(r.recording_id)), NULL,
  CASE f.state WHEN 1 THEN 'function_metadata_unavailable' ELSE 'function_undefined' END,
  'warning', 'function', __btel_sid(r.recording_id, 'f', f.function_id),
  CASE f.state WHEN 1 THEN 'the producer could not look up this function; its name is unavailable'
    ELSE 'a call path references this function but no definition is indexed yet' END
FROM main.function_def f INDEXED BY function_def_unresolved
JOIN main.recording r ON r.rec = f.rec WHERE f.state < 2
UNION ALL
SELECT lower(hex(r.recording_id)), NULL, 'call_path_undefined', 'warning', 'call_path',
  __btel_sid(r.recording_id, 'p', p.call_path_id),
  'calls, aggregates or other paths reference this calling context but no definition is indexed yet'
FROM main.call_path p INDEXED BY call_path_undepthed
JOIN main.recording r ON r.rec = p.rec WHERE p.depth IS NULL AND p.defined = 0
UNION ALL
SELECT lower(hex(r.recording_id)), NULL,
  CASE WHEN t.defined = 0 THEN 'thread_undefined' ELSE 'thread_parent_unresolved' END,
  'warning', 'thread', __btel_pubid(r.recording_id, t.thread_id),
  CASE WHEN t.defined = 0
    THEN 'evidence references this thread but its definition is not indexed yet'
    ELSE 'the recorded parent is not an indexed thread or retained call with a known execution; its execution is unknown' END
FROM main.thread t INDEXED BY thread_unresolved
JOIN main.recording r ON r.rec = t.rec WHERE t.root_id IS NULL
UNION ALL
SELECT lower(hex(r.recording_id)), NULL,
  CASE WHEN e.defined = 0 THEN 'clock_epoch_undefined' ELSE 'clock_invalidated' END,
  'warning', 'clock', __btel_sid(r.recording_id, 'c', e.epoch_id),
  CASE WHEN e.defined = 0 THEN 'a clock epoch is referenced but not defined; its timings are unavailable'
    ELSE 'the clock was observed ' || CASE s.status WHEN 2 THEN 'restored' WHEN 3 THEN 'discontinuous'
      WHEN 4 THEN 'uncertain' ELSE 'changing mode' END
      || '; durations on it are unavailable, counts are kept' END
FROM main.epoch e JOIN main.recording r ON r.rec = e.rec
LEFT JOIN main.epoch_state s ON s.rec = e.rec AND s.epoch_id = e.epoch_id
WHERE e.defined = 0 OR s.status > 1
UNION ALL
SELECT lower(hex(r.recording_id)), NULL, 'announcement_pending', 'warning', 'recording', NULL,
  COUNT(*) || ' completed call(s) expect an input announcement not indexed yet; their args are pending'
FROM main.call c INDEXED BY call_pending JOIN main.recording r ON r.rec = c.rec
WHERE c.needs_announcement = 1 AND c.announced_sequence IS NULL GROUP BY c.rec",
};

pub const RECORDING_FILES: Relation = Relation {
    name: "recording_files",
    doc: "Recording files the index applied or rejected: the ledger that lets a refresh skip unchanged files.",
    columns: &[
        col("recording_id", "text", ""),
        col("sequence", "integer", ""),
        col(
            "state",
            "text",
            "applied, or rejected (invalid; stops indexing at this file)",
        ),
        col("byte_length", "integer", ""),
        col(
            "content_fingerprint",
            "text",
            "reader-computed hash of an applied file; not a producer checksum",
        ),
        col("reason", "text", "why a file was rejected"),
    ],
    view: "
CREATE TEMP VIEW recording_files AS
SELECT lower(hex(r.recording_id)) AS recording_id, l.sequence, 'applied' AS state,
  l.size AS byte_length, lower(hex(l.content_hash)) AS content_fingerprint, NULL AS reason
FROM main.ledger l JOIN main.recording r ON r.rec = l.rec
UNION ALL
SELECT lower(hex(r.recording_id)), j.sequence, 'rejected', j.size, NULL, j.reason
FROM main.rejected j JOIN main.recording r ON r.rec = j.rec",
};

/// What the old tracer answered and how much of it current recordings
/// support. Also served as the `capabilities` relation.
pub const CAPABILITY_ROWS: &[(&str, &str, &str, &str)] = &[
    (
        "executions",
        "supported",
        "executions",
        "Host invocations with root outcome, timing, entry function and summary counts.",
    ),
    (
        "threads and spawn tree",
        "supported",
        "threads",
        "Root and spawned threads, parent thread or call, spawn context and lifetime.",
    ),
    (
        "calling contexts",
        "supported",
        "call_paths",
        "Callee, visible caller, bytecode pc and its call-site line, parent context and depth.",
    ),
    (
        "completed call counts",
        "supported",
        "call_path_stats, function_stats",
        "Completed invocations over every recorded call. Not invocation starts: an unfinished call is not counted.",
    ),
    (
        "recursion-aware time",
        "supported",
        "call_path_stats",
        "inclusive_ns without recursive double counting, direct_child_ns, await_ns and self_ns with a state.",
    ),
    (
        "retained calls and captured values",
        "supported",
        "calls",
        "Retained calls with args, output and error values; filter and render with brackets.",
    ),
    (
        "structured value equality",
        "partial",
        "calls",
        "= and != compare captured lists/maps/classes by content; baml_value_json supplies a plain JSON literal. Missing/truncated evidence, cycles, opaque values and comparison limits remain explicit. Structured ordering and value-based DISTINCT/grouping are unsupported.",
    ),
    (
        "errored calls",
        "supported",
        "error_calls",
        "Retained calls that ended in an error, with their error values and ancestry through parent_call_id.",
    ),
    (
        "function metadata and parameters",
        "supported",
        "function_definitions, function_parameters",
        "Recorded names, kinds, origins, definition spans and parameter slots.",
    ),
    (
        "clock interpretation",
        "supported",
        "clocks",
        "Clock sources, conversions, calibration, UTC anchors and observed validity.",
    ),
    (
        "evidence problems",
        "supported",
        "issues, recordings, recording_files",
        "Gaps, invalid files, conflicts, unresolved references and invalidated clocks.",
    ),
    (
        "entry function",
        "partial",
        "executions",
        "Known only when the root thread recorded exactly one top-level call.",
    ),
    (
        "source locations",
        "partial",
        "call_paths, threads, calls, error_raises, error_frames",
        "Call, spawn and raise sites resolve through the recording's own source maps (format minor 2). A direct recursive re-entry has no site of its own. Older recordings have definition spans only. Locations are the recorded program's; today's file may differ.",
    ),
    (
        "failure ancestry",
        "supported",
        "error_raises, error_frames, error_call_links, error_calls",
        "The stack when each raise started, and which retained calls it failed. Timing-only calls have no ids; recursion in the stack is not collapsed.",
    ),
    (
        "live recordings",
        "partial",
        "recordings",
        "Unsealed recordings answer from their indexed prefix. No liveness is recorded: an incomplete execution is not proof it is still running.",
    ),
    (
        "throw occurrences (old errors relation)",
        "partial",
        "error_occurrences, error_raises",
        "Every raise has its own id, including throws caught where they were thrown. A rethrow or await names its origin only when a catch landing or future link proves it; otherwise it is ambiguous or unresolved. An UnknownError conversion never establishes a proven link: when it carries an earlier throw's context it is unresolved (source_not_recorded) and keeps that trace as weaker evidence; otherwise it is a fresh raise. Native and host failures are located at the BAML call that entered them. Recordings before format minor 2 have none.",
    ),
    (
        "population outcome counts",
        "supported",
        "call_path_nodes, call_path_stats, function_stats",
        "Completed success/error/cancellation counts from aggregate outcomes, including timing-only calls. Older or mixed deltas have NULL counts and an explicit outcome_state. Unfinished/hidden calls are not counted.",
    ),
    (
        "invocation starts and active calls",
        "unsupported",
        "",
        "Aggregates are written at completion; unfinished invocations are not counted.",
    ),
    (
        "latency distributions",
        "unsupported",
        "",
        "Only sums and counts are recorded for aggregated calls: no percentiles, minimum or maximum. Retained calls have individual durations.",
    ),
    (
        "await counts",
        "unsupported",
        "",
        "Await time is recorded, the number of awaits is not.",
    ),
    ("thread names", "unsupported", "", "Not recorded."),
    (
        "producer process and version (old processes relation)",
        "unsupported",
        "",
        "Recordings carry no process, pid, engine or version identity.",
    ),
    (
        "capture loss reasons (old health relation)",
        "unsupported",
        "",
        "The recording does not say why a value was not captured or how many events the producer dropped.",
    ),
    (
        "sealed recordings and final clocks",
        "supported",
        "recordings, clocks",
        "A normal shutdown ends the recording once every recorded run's clock settled, and marks those clocks final. Older recordings, and recordings that stop before their end is written (a crash, a failure, or runs still attached at shutdown), stay unsealed. Sealed is not proof every capture or cloud upload arrived.",
    ),
    (
        "old store internals (store_files, value_index)",
        "unsupported",
        "recording_files",
        "Replaced by recording_files; captured blobs are not enumerated.",
    ),
];

pub const CAPABILITIES: Relation = Relation {
    name: "capabilities",
    doc: "What current recordings can answer, compared with the old tracer: supported, partial or unsupported, and where to look.",
    columns: &[
        col("capability", "text", ""),
        col("support", "text", "supported, partial, unsupported"),
        col("relation", "text", "where to query it"),
        col("detail", "text", ""),
    ],
    view: "",
};

fn capabilities_view() -> String {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let rows: Vec<String> = CAPABILITY_ROWS
        .iter()
        .map(|(capability, support, relation, detail)| {
            format!(
                "({}, {}, {}, {})",
                quote(capability),
                quote(support),
                quote(relation),
                quote(detail)
            )
        })
        .collect();
    format!(
        "CREATE TEMP VIEW capabilities (capability, support, relation, detail) AS VALUES {}",
        rows.join(",\n  ")
    )
}

pub const RELATIONS: &[Relation] = &[
    RECORDINGS,
    EXECUTIONS,
    THREADS,
    CALLS,
    ERROR_CALLS,
    ERROR_OCCURRENCES,
    ERROR_RAISES,
    ERROR_FRAMES,
    ERROR_CALL_LINKS,
    CALL_PATHS,
    CALL_PATH_NODES,
    CALL_PATH_STATS,
    HOT_CALL_PATHS,
    FUNCTION_STATS,
    FUNCTION_DEFINITIONS,
    FUNCTION_PARAMETERS,
    CLOCKS,
    ISSUES,
    RECORDING_FILES,
    CAPABILITIES,
];

/// Old relation names with no current equivalent, and what to use instead.
pub const RETIRED: &[(&str, &str)] = &[
    (
        "errors",
        "query error_occurrences for distinct errors, error_raises for every throw/rethrow with its origin, error_frames for stacks, and error_call_links or error_calls for the retained calls that failed",
    ),
    (
        "health",
        "producer loss counters are not recorded; query issues for problems the reader found",
    ),
    (
        "processes",
        "recordings carry no process identity; query recordings",
    ),
    ("store_files", "query recording_files"),
    (
        "value_index",
        "captured blobs are not enumerated; values resolve lazily through calls.args, output and error",
    ),
    (
        "cct_population",
        "query call_path_stats (counts are completed calls, not starts)",
    ),
];

pub fn relation(name: &str) -> Option<&'static Relation> {
    RELATIONS
        .iter()
        .find(|relation| relation.name.eq_ignore_ascii_case(name))
}

pub fn retired(name: &str) -> Option<&'static str> {
    RETIRED
        .iter()
        .find(|(old, _)| old.eq_ignore_ascii_case(name))
        .map(|(_, advice)| *advice)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rusqlite::Connection;

    /// Without statistics, SQLite may look rows up by recording alone inside
    /// a loop, rescanning the recording's history once per row. No relation
    /// may plan that way: every inner lookup narrows past `rec`, and full
    /// scans only drive the outermost loop of a (non-correlated) query.
    #[test]
    fn relations_never_rescan_a_recording_per_row() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::ensure_schema(&mut conn).unwrap();
        crate::functions::register(&conn, &crate::functions::ContextSlot::default()).unwrap();
        for relation in super::RELATIONS {
            conn.execute_batch(&relation.create_sql()).unwrap();
            let plan: Vec<(i64, i64, String)> = conn
                .prepare(&format!(
                    "EXPLAIN QUERY PLAN SELECT * FROM {}",
                    relation.name
                ))
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let detail = |id: i64| {
                plan.iter()
                    .find(|(step, _, _)| *step == id)
                    .map_or("", |(_, _, detail)| detail.as_str())
            };
            let mut outer_loops = HashSet::new();
            for (_, parent, step) in &plan {
                if !(step.starts_with("SCAN ") || step.starts_with("SEARCH ")) {
                    continue;
                }
                let inner =
                    !outer_loops.insert(*parent) || detail(*parent).starts_with("CORRELATED");
                let rescans = step.ends_with("(rec=?)") || (inner && step.starts_with("SCAN "));
                assert!(
                    !rescans,
                    "{} rescans per row: {step}\n{plan:#?}",
                    relation.name
                );
            }
        }
    }

    /// Every relation's documented columns are exactly its view's columns.
    #[test]
    fn documented_columns_match_views() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::ensure_schema(&mut conn).unwrap();
        crate::functions::register(&conn, &crate::functions::ContextSlot::default()).unwrap();
        for relation in super::RELATIONS {
            conn.execute_batch(&relation.create_sql()).unwrap();
            let statement = conn
                .prepare(&format!("SELECT * FROM {}", relation.name))
                .unwrap();
            let actual: Vec<&str> = statement.column_names();
            let documented: Vec<&str> = relation.columns.iter().map(|c| c.name).collect();
            assert_eq!(actual, documented, "{}", relation.name);
        }
    }
}
