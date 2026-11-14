//! The public relations: `processes`, `profiler`, `spans` and
//! `span_announcements`, the tables local `baml query` and the cloud share.
//! Each is a TEMP view over the internal tables, created on every query
//! connection so its definition always matches this binary.
//!
//! IDs render as text. `process_id` is 32 hex digits. Spans (function calls,
//! futures and network requests) are `<recording_id>:<n>`, unique across
//! recordings, the pair `trace.SpanId` holds. Profiler nodes are 16 hex
//! digits hashed from the function names on their path, so the same node
//! has the same ID in every process. Timestamps are RFC 3339 UTC; durations are nanoseconds, NULL
//! when the clock evidence cannot support them. `type_args`, `input_args`, `output_value`, `network_event_values`,
//! `error_value`, `context_metadata`, `status_history`, `self_usage`
//! and `total_usage` are BAML values: navigate them with `['key']` and
//! `[index]`.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Column {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub sql_type: &'static str,
    /// A BAML value column: supports bracket navigation, rendered on output.
    pub value: bool,
    pub doc: &'static str,
    /// What to call the column in front of a person, where its name is not that already:
    /// `User` for `context_distinct_id`. Empty for a column whose name reads as it is.
    #[serde(skip_serializing_if = "str::is_empty")]
    pub heading: &'static str,
    /// What to show a person for each value the column takes. SQL filters on the value, so
    /// results keep it; a report or a dashboard shows the label. Empty for a column that is
    /// not one of a few known values.
    #[serde(skip_serializing_if = "<[ValueLabel]>::is_empty")]
    pub labels: &'static [ValueLabel],
}

/// One value of a column, the word to show a person for it, and what it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ValueLabel {
    pub value: &'static str,
    pub label: &'static str,
    pub meaning: &'static str,
}

const fn shown(value: &'static str, label: &'static str, meaning: &'static str) -> ValueLabel {
    ValueLabel {
        value,
        label,
        meaning,
    }
}

impl Column {
    const fn heading(self, heading: &'static str) -> Self {
        Self { heading, ..self }
    }
    const fn labels(self, labels: &'static [ValueLabel]) -> Self {
        Self { labels, ..self }
    }
}

/// A span's `status`. `user_error` reads as a mistake of the program's user and is not one,
/// which is why a page says Failed.
pub const SPAN_STATUS_LABELS: &[ValueLabel] = &[
    shown("return", "Succeeded", "the call returned a value"),
    shown(
        "user_error",
        "Failed",
        "the call ended with an error the program threw; not a mistake of the program's user",
    ),
    shown(
        "panic_error",
        "Crashed",
        "the call panicked: a bug such as an index out of range or a failed assertion",
    ),
    shown(
        "cancel_error",
        "Cancelled",
        "the call was stopped before it finished, as by a timeout or the end of the work that started it",
    ),
    shown(
        "closed",
        "Ended",
        "an explicit operation was closed without saying whether it succeeded",
    ),
];

/// A process's `status`. `draining` and `dead` come from liveness reports, which only BAML
/// Cloud has.
pub const PROCESS_STATUS_LABELS: &[ValueLabel] = &[
    shown("running", "Running", "the program is still running"),
    shown("success", "Succeeded", "the program exited normally"),
    shown("error", "Failed", "the program exited with an error"),
    shown("panicked", "Crashed", "the program ended with a panic"),
    shown(
        "unknown",
        "Unknown",
        "the recording ended without saying how the program exited",
    ),
    shown(
        "draining",
        "Finishing",
        "BAML Cloud only: the program stopped taking work and is sending the rest of its recording",
    ),
    shown(
        "dead",
        "Stopped responding",
        "BAML Cloud only: the program was running and has not been heard from for several minutes",
    ),
];

pub const SPAN_TYPE_LABELS: &[ValueLabel] = &[
    shown("function", "Function", "a call of a function"),
    shown(
        "future",
        "Task",
        "work that runs beside its caller: a spawned future, or a host's root call",
    ),
    shown(
        "network_span",
        "Network request",
        "an HTTP request the runtime made",
    ),
];

pub const NODE_TYPE_LABELS: &[ValueLabel] = &[
    shown("function", "Function", "a function on the code path"),
    shown("future", "Task", "a spawned future on the code path"),
];

/// How to word what a row is and how to print the shapes every relation shares, for whoever
/// builds a page from query results. Printed after the relations by `baml local recordings schema`.
pub const PRESENTING: &[&str] = &[
    "Say call, run or step for a span, run for a root call (parent_span_id IS NULL), and task for a future.",
    "Durations are nanoseconds: show 1500000000 as 1.5 s and 42000000 as 42 ms. Show a time in the reader's time zone.",
    "Names are fully qualified: show user.tools.refund_order as refund_order, with user.tools beside it when two names collide.",
    "Show an id shortened, and keep the whole id for links and filters.",
    "A NULL is not a zero or an empty string: say what is missing (\"Not recorded\", \"Still running\").",
];

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
        self.view.to_owned()
    }
}

const fn col(name: &'static str, sql_type: &'static str, doc: &'static str) -> Column {
    Column {
        name,
        sql_type,
        value: false,
        doc,
        heading: "",
        labels: &[],
    }
}
const fn value(name: &'static str, doc: &'static str) -> Column {
    Column {
        name,
        sql_type: "baml_value",
        value: true,
        doc,
        heading: "",
        labels: &[],
    }
}

pub const PROCESSES: Relation = Relation {
    name: "processes",
    doc: "One row per process that ran BAML: every engine it started shares the row.",
    columns: &[
        col(
            "process_id",
            "text",
            "32 hex digits; a recording from before process IDs is its own process",
        ).heading("Process"),
        col(
            "baml_source_code_content_id",
            "text",
            "CAS id of the BAML sources it ran: the root blob of a map<path, content> whose large files are blobs of their own; NULL when not recorded",
        ).heading("Revision"),
        col("baml_version", "text", ""),
        col(
            "host",
            "text",
            "what ran BAML: baml (the CLI), lsp, pack, python, ...",
        ),
        col(
            "command",
            "text",
            "the process's arguments, space-separated; debugging only",
        ),
        col(
            "context_distinct_id",
            "text",
            "immutable launch identity; NULL when absent or unavailable",
        ).heading("User"),
        value(
            "context_metadata",
            "immutable launch metadata; unavailable in older recordings",
        ),
        col(
            "status",
            "text",
            "running, success, error, panicked, or unknown (the host did not report the OS process's exit outcome)",
        ).labels(PROCESS_STATUS_LABELS),
        value(
            "status_history",
            "[{status, timestamp}]: running from the process's start, then its reported completion or exit outcome",
        ),
        col(
            "last_updated",
            "text",
            "when it ended, else the newest recording file indexed",
        ),
    ],
    view: "
CREATE TEMP VIEW processes AS
SELECT lower(hex(g.process_id)) AS process_id,
  -- `hex(NULL)` is the empty string, which would read as a recorded id.
  IIF(g.source_cas IS NULL, NULL, lower(hex(g.source_cas))) AS baml_source_code_content_id,
  g.baml_version, g.host,
  (SELECT group_concat(a.value, ' ') FROM json_each(g.command) a) AS command,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(IIF(g.initial_context_cas IS NULL, 0, 2),
    g.initial_context_cas, cx.state) AS context_metadata,
  COALESCE(g.ended, IIF(g.open = 0, 'unknown', 'running')) AS status,
  __btel_status_history(g.started_ns, g.ended, g.ended_ns) AS status_history,
  __btel_ns_utc(COALESCE(g.ended_ns, g.updated_ns)) AS last_updated
FROM (
  SELECT COALESCE(r.process_id, r.recording_id) AS process_id,
    MAX(r.source_cas) AS source_cas, MAX(r.baml_version) AS baml_version,
    MAX(r.initial_context_cas) AS initial_context_cas,
    MAX(r.host) AS host, MAX(r.command) AS command,
    MIN(r.process_started_ns) AS started_ns,
    CASE MAX(CASE WHEN r.process_end_status = 4 THEN 0 ELSE r.process_end_status END)
      WHEN 0 THEN 'unknown' WHEN 1 THEN 'success' WHEN 2 THEN 'error'
      WHEN 3 THEN 'panicked' END AS ended,
    -- A known outcome's time wins over a later unknown completion's.
    COALESCE(MAX(IIF(r.process_end_status = 4, NULL, r.process_end_ns)),
      MAX(r.process_end_ns)) AS ended_ns,
    SUM(r.terminal_sequence IS NULL) AS open,
    MAX(r.updated_ns) AS updated_ns
  FROM main.recording r
  GROUP BY COALESCE(r.process_id, r.recording_id)
) g LEFT JOIN main.context_snapshot cx ON cx.cas = g.initial_context_cas",
};

pub const PROFILER: Relation = Relation {
    name: "profiler",
    doc: "One row per calling-context tree node of each process whose host reported completion, merged over its call sites, threads and engines. Empty until completion is recorded; the OS process's exit outcome can remain unknown.",
    columns: &[
        col(
            "profiler_node_id",
            "text",
            "16 hex digits; the same code path has the same ID in every process",
        ).heading("Code path"),
        col("process_id", "text", "").heading("Process"),
        col(
            "process_start_time",
            "text",
            "when the process started, the same on every row of a process; NULL when not recorded",
        ),
        col(
            "process_end_time",
            "text",
            "when the process ended, the same on every row of a process: the end it reported, else the newest recording file indexed, else its start. WHERE process_end_time >= '2026-01-01' bounds code paths by time without joining processes",
        ),
        col(
            "baml_source_code_content_id",
            "text",
            "the source revision the process ran, the same on every row of a process and equal to its processes.baml_source_code_content_id; NULL when not recorded. WHERE baml_source_code_content_id = '<id>' picks one revision's code paths without joining processes",
        ).heading("Revision"),
        col(
            "parent_profiler_node_id",
            "text",
            "NULL for a top-level call",
        ).heading("Called from"),
        col(
            "node_type",
            "text",
            "future for a spawned future (its children are what it ran), else function",
        ).labels(NODE_TYPE_LABELS),
        col(
            "function_name",
            "text",
            "fully qualified, package included; for a future, the function it runs. NULL when not recorded",
        ),
        col(
            "total_time",
            "integer",
            "nanoseconds in completed invocations; direct recursion counted once. A future's runs from when it began running, not from when it was scheduled",
        ),
        col(
            "self_time",
            "integer",
            "total_time minus the synchronous callees' total_time",
        ),
        col(
            "io_total_time",
            "integer",
            "nanoseconds in sysops (baml.fs.read, baml.http.send, ...), callees included",
        ),
        col(
            "io_self_time",
            "integer",
            "nanoseconds in sysops this node called itself",
        ),
        col(
            "invocation_count",
            "integer",
            "return_count + error_count + future_cancel_count + missing_count",
        ),
        col("return_count", "integer", ""),
        col(
            "future_cancel_count",
            "integer",
            "futures cancelled (or unwound by baml.sys.exit); 0 for a function",
        ),
        col(
            "error_count",
            "integer",
            "panic_error_count + nonpanic_error_count",
        ),
        col(
            "panic_error_count",
            "integer",
            "ended by a panic (cancellations excluded); NULL for recordings from before panics were recorded",
        ),
        col("nonpanic_error_count", "integer", "ended by a thrown error"),
        col(
            "missing_count",
            "integer",
            "functions that never finished: interrupted by a cancellation or baml.sys.exit; 0 for a future",
        ),
    ],
    view: "
CREATE TEMP VIEW profiler AS
SELECT __btel_hex(n.node_id) AS profiler_node_id,
  lower(hex(n.process_id)) AS process_id,
  __btel_ns_utc(g.started_ns) AS process_start_time,
  __btel_ns_utc(COALESCE(g.ended_ns, g.updated_ns, g.started_ns)) AS process_end_time,
  -- `hex(NULL)` is the empty string, which would read as a recorded id.
  IIF(g.source_cas IS NULL, NULL, lower(hex(g.source_cas))) AS baml_source_code_content_id,
  __btel_hex(n.parent_node_id) AS parent_profiler_node_id,
  n.node_type,
  n.function_name,
  n.total_ns AS total_time, n.self_ns AS self_time,
  n.io_total_ns AS io_total_time, n.io_self_ns AS io_self_time,
  n.invocation_count, n.return_count, n.future_cancel_count, n.error_count,
  n.panic_error_count, n.nonpanic_error_count, n.missing_count
FROM main.profile_node n
-- The process's times and source revision, as `processes` derives them from
-- its recordings.
LEFT JOIN (
  SELECT COALESCE(r.process_id, r.recording_id) AS process_id,
    MAX(r.source_cas) AS source_cas,
    MIN(r.process_started_ns) AS started_ns,
    COALESCE(MAX(IIF(r.process_end_status = 4, NULL, r.process_end_ns)),
      MAX(r.process_end_ns)) AS ended_ns,
    MAX(r.updated_ns) AS updated_ns
  FROM main.recording r
  GROUP BY COALESCE(r.process_id, r.recording_id)
) g ON g.process_id = n.process_id",
};

const SPAN_STATUS_DOC: &str = "return, user_error, panic_error (a panic other than a cancellation), cancel_error (cancelled, or unwound by baml.sys.exit), or closed (an explicit operation ended without an outcome)";

const TYPE_ARGS_DOC: &str = "the type arguments a generic function was called with, by type-parameter name: type_args['T'] is {\"$type\": {\"type\": \"class\", \"name\": \"user.Resume\"}}, and a list of them {\"$type\": {\"type\": \"list\", \"item\": …}}. A method's include its class's (Box<int>.map<string>: {T: int, U: string}), an interface method's its Self, a lambda's its enclosing function's. Captured exactly when input_args is. NULL for a non-generic function, a span whose inputs aren't captured, a future or a network span";

const NETWORK_EVENTS_DOC: &str = "[{event_name, payload, timestamp}] in time order. connection: {status, headers}. data: a whole body, or one server-sent event as {event, data, id}; null when bodies are not recorded. end (the stream ended on the wire), await (the program finished reading), close (it closed the stream early) and drop: null. Other names are other protocols' events. network_event_values[0]['payload'] reads only that payload. NULL for functions and futures";

pub const SPANS: Relation = Relation {
    name: "spans",
    doc: "One row per completed span: a retained function call (LLM functions, $trace spans, promoted calls), a future (a spawned future, or a host's root call), or a network span (an HTTP request the runtime made).",
    columns: &[
        col("span_id", "text", "<recording_id>:<n>"),
        col("span_type", "text", "function, future or network_span").labels(SPAN_TYPE_LABELS),
        col(
            "span_name",
            "text",
            "the function's fully qualified name; for a future its name, else the function it runs; for a network span its method and URL, with query values hashed",
        ).heading("Name"),
        col("process_id", "text", "").heading("Process"),
        col(
            "future_id",
            "text",
            "the future a function or network span ran on; for a future, the future that spawned it",
        ).heading("Task"),
        col(
            "profiler_node_id",
            "text",
            "its node in the process's profiler (for a network span, the function that made the request); NULL while a function on its path is unknown",
        ).heading("Code path"),
        col(
            "parent_span_id",
            "text",
            "the enclosing span; NULL for a root call",
        ).heading("Called by"),
        col("status", "text", SPAN_STATUS_DOC).labels(SPAN_STATUS_LABELS),
        col(
            "scheduled_time",
            "text",
            "RFC 3339 UTC; futures only: when it was spawned, before it waited to start (a Limit). NULL for a function, and for futures recorded before this was recorded",
        ),
        col(
            "start_time",
            "text",
            "RFC 3339 UTC; for a future, when it began running (when it was cancelled, if it never ran)",
        ),
        col("end_time", "text", "RFC 3339 UTC"),
        col("duration", "integer", "nanoseconds, start_time to end_time"),
        value("type_args", TYPE_ARGS_DOC),
        value(
            "input_args",
            "captured arguments by name or position: input_args['customer'], input_args[0]; for a network span the request, {request: {method, url, headers, body}}; NULL for futures",
        ),
        value(
            "output_value",
            "captured return value of a span that returned; NULL for a network span, whose response is in its events",
        ),
        value("network_event_values", NETWORK_EVENTS_DOC),
        value(
            "error_value",
            "the baml.errors.Context of a failed invocation: of a function, the error it threw; of a future, the error that escaped it (recorded since format minor 14); for a manual operation, the value passed to fail(). A span records it by default; a cancellation only on request",
        ),
        col(
            "context_distinct_id",
            "text",
            "entry-time identity; NULL when absent or unavailable",
        ).heading("User"),
        value(
            "context_metadata",
            "entry-time metadata; explicit empty context is an empty map, unknown context is unavailable",
        ),
        value(
            "self_usage",
            "model usage of the calls this span's own invocation made: a list with one map per provider and model of provider, model (null when no response named one), calls (the calls that reported usage), input_tokens (full-rate), output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens. A count is null when no call reported it. Empty unless the span is an ai.Client.invoke implementation or a stream's settle step",
        ),
        value(
            "total_usage",
            "model usage of every call under this span, itself included, as in self_usage. Counts calls whose own spans were not recorded. Empty without usage",
        ),
        col(
            "span_reason",
            "text",
            "policy_promoted (recorded only after it finished, by a policy), or other",
        ),
    ],
    view: "
CREATE TEMP VIEW spans AS
SELECT __btel_pubid(r.recording_id, c.call_id) AS span_id,
  'function' AS span_type,
  f.fqn AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, c.thread_id) AS future_id,
  __btel_hex(p.node_id) AS profiler_node_id,
  __btel_pubid(r.recording_id, c.parent_id) AS parent_span_id,
  CASE c.outcome WHEN 1 THEN 'return' WHEN 2 THEN IIF(c.panicked, 'panic_error', 'user_error')
    WHEN 3 THEN 'cancel_error' WHEN 4 THEN 'closed' END AS status,
  NULL AS scheduled_time,
  __btel_utc(c.entered_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS start_time,
  __btel_utc(c.exited_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS end_time,
  __btel_duration(c.entered_ticks, c.exited_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration,
  __btel_ref(2, c.type_args_cas, NULL,
    c.needs_announcement = 1 AND c.announced_sequence IS NULL) AS type_args,
  -- Host adapters capture a named map (Python) or positional list (Node) as
  -- a value root. BAML functions capture FunctionArgs with a slot layout.
  __btel_ref(IIF(c.manual = 1 OR f.definition_key LIKE 'host:%', 2, 1), c.inputs_cas, f.argument_names,
    c.needs_announcement = 1 AND c.announced_sequence IS NULL) AS input_args,
  IIF(c.outcome = 1, __btel_ref(2, c.value_cas, NULL, 0), NULL) AS output_value,
  NULL AS network_event_values,
  IIF(c.outcome IN (2, 3), __btel_ref(IIF(c.manual = 1, 2, 3), c.value_cas, NULL, 0), NULL) AS error_value,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  -- A span's own usage is its own calls' only when its function made them.
  __btel_usage_value(IIF(f.model_invoke = 1, c.usage, NULL)) AS self_usage,
  __btel_usage_value(c.total_usage) AS total_usage,
  CASE WHEN c.late = 1 THEN 'policy_promoted' ELSE 'other' END AS span_reason,
  c.rec AS __span_id_rec, c.call_id AS __span_id_key,
  c.rec AS __future_id_rec, c.thread_id AS __future_id_key,
  c.rec AS __parent_span_id_rec, c.parent_id AS __parent_span_id_key
FROM main.call c
JOIN main.recording r ON r.rec = c.rec
JOIN main.event_context ec ON ec.rec = c.rec AND ec.node_id = c.call_id AND ec.slot = 0
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path p ON p.rec = c.rec AND p.call_path_id = c.call_path_id
LEFT JOIN main.function_def f ON f.rec = c.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.thread t ON t.rec = c.rec AND t.thread_id = c.thread_id
LEFT JOIN main.epoch e ON e.rec = c.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = c.rec AND s.epoch_id = t.epoch_id
WHERE c.outcome IS NOT NULL
UNION ALL
SELECT __btel_pubid(r.recording_id, t.thread_id) AS span_id,
  'future' AS span_type,
  COALESCE(t.name, sf.fqn, (SELECT ef.fqn FROM main.call_path ep
     JOIN main.function_def ef ON ef.rec = ep.rec AND ef.function_id = ep.callee_function_id
     WHERE ep.rec = t.rec AND ep.call_path_id = t.entry_call_path_id)) AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, COALESCE(pt.thread_id, pc.thread_id)) AS future_id,
  __btel_hex(COALESCE(sp.node_id, (SELECT ep.node_id FROM main.call_path ep
     WHERE ep.rec = t.rec AND ep.call_path_id = t.entry_call_path_id))) AS profiler_node_id,
  __btel_pubid(r.recording_id, t.parent_id) AS parent_span_id,
  CASE t.outcome WHEN 1 THEN 'return' WHEN 2 THEN IIF(t.panicked, 'panic_error', 'user_error')
    WHEN 3 THEN 'cancel_error' END AS status,
  -- A root future runs at once; a spawned one records when it began running
  -- (format minor 6), and before that only when it was scheduled.
  IIF(t.ran_ticks IS NOT NULL OR t.parent_id IS NULL,
    __btel_utc(t.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
      e.multiplier, e.shift), NULL) AS scheduled_time,
  __btel_utc(COALESCE(t.ran_ticks, t.started_ticks), IIF(e.conflict = 0, e.utc_ticks, NULL),
    e.utc_unix_ns, e.multiplier, e.shift) AS start_time,
  __btel_utc(t.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS end_time,
  __btel_duration(COALESCE(t.ran_ticks, t.started_ticks), t.completed_ticks, e.multiplier,
    e.shift, s.status, IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration,
  NULL AS type_args,
  NULL AS input_args,
  NULL AS output_value,
  NULL AS network_event_values,
  -- The error that escaped the future's outermost frame (format minor 14).
  IIF(t.outcome = 2, __btel_ref(3, t.error_cas, NULL, 0), NULL) AS error_value,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  __btel_usage_value(NULL) AS self_usage,
  __btel_usage_value(t.total_usage) AS total_usage,
  'other' AS span_reason,
  t.rec AS __span_id_rec, t.thread_id AS __span_id_key,
  t.rec AS __future_id_rec, COALESCE(pt.thread_id, pc.thread_id) AS __future_id_key,
  t.rec AS __parent_span_id_rec, t.parent_id AS __parent_span_id_key
FROM main.thread t
JOIN main.recording r ON r.rec = t.rec
JOIN main.event_context ec ON ec.rec = t.rec AND ec.node_id = t.thread_id AND ec.slot = 0
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path sp ON sp.rec = t.rec AND sp.call_path_id = t.spawn_call_path_id
LEFT JOIN main.function_def sf ON sf.rec = t.rec AND sf.function_id = sp.callee_function_id
LEFT JOIN main.thread pt ON pt.rec = t.rec AND pt.thread_id = t.parent_id
LEFT JOIN main.call pc ON pc.rec = t.rec AND pc.call_id = t.parent_id
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = t.rec AND s.epoch_id = t.epoch_id
WHERE t.outcome IS NOT NULL
UNION ALL
SELECT __btel_pubid(r.recording_id, n.span_id) AS span_id,
  'network_span' AS span_type,
  n.method || ' ' || n.url AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, n.thread_id) AS future_id,
  __btel_hex(p.node_id) AS profiler_node_id,
  __btel_pubid(r.recording_id, n.parent_id) AS parent_span_id,
  CASE n.outcome WHEN 1 THEN 'return' WHEN 2 THEN IIF(n.panicked, 'panic_error', 'user_error')
    WHEN 3 THEN 'cancel_error' END AS status,
  NULL AS scheduled_time,
  __btel_utc(n.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS start_time,
  __btel_utc(n.completed_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS end_time,
  __btel_duration(n.started_ticks, n.completed_ticks, e.multiplier, e.shift, s.status,
    IIF(e.rec IS NULL, 0, 1 + e.conflict)) AS duration,
  NULL AS type_args,
  -- The request is due while only the completion is indexed.
  __btel_ref(2, n.request_cas, NULL, n.defined = 0) AS input_args,
  NULL AS output_value,
  (SELECT __btel_events(v.at_ticks, v.seq, v.name, __btel_utc(v.at_ticks,
       IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns, e.multiplier, e.shift),
       v.payload_cas)
   FROM main.network_event v WHERE v.rec = n.rec AND v.span_id = n.span_id) AS network_event_values,
  IIF(n.outcome IN (2, 3), __btel_ref(3, n.error_cas, NULL, 0), NULL) AS error_value,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  __btel_usage_value(NULL) AS self_usage,
  __btel_usage_value(NULL) AS total_usage,
  'other' AS span_reason,
  n.rec AS __span_id_rec, n.span_id AS __span_id_key,
  n.rec AS __future_id_rec, n.thread_id AS __future_id_key,
  n.rec AS __parent_span_id_rec, n.parent_id AS __parent_span_id_key
FROM main.network_span n
JOIN main.recording r ON r.rec = n.rec
-- The context of the frame that sent the request, from its announcement.
JOIN main.event_context ec ON ec.rec = n.rec AND ec.node_id = n.span_id AND ec.slot = 0
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path p ON p.rec = n.rec AND p.call_path_id = n.call_path_id
LEFT JOIN main.thread t ON t.rec = n.rec AND t.thread_id = n.thread_id
LEFT JOIN main.epoch e ON e.rec = n.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
LEFT JOIN main.epoch_state s ON s.rec = n.rec AND s.epoch_id = t.epoch_id
WHERE n.outcome IS NOT NULL",
};

pub const SPAN_ANNOUNCEMENTS: Relation = Relation {
    name: "span_announcements",
    doc: "One row per span known to have started, finished or not.",
    columns: &[
        col("span_id", "text", "<recording_id>:<n>"),
        col("span_type", "text", "function, future or network_span").labels(SPAN_TYPE_LABELS),
        col("span_name", "text", "as in spans").heading("Name"),
        col("process_id", "text", "").heading("Process"),
        col("future_id", "text", "as in spans").heading("Task"),
        col("profiler_node_id", "text", "as in spans").heading("Code path"),
        col("parent_span_id", "text", "as in spans").heading("Called by"),
        col("start_time", "text", "RFC 3339 UTC"),
        value("type_args", "as in spans"),
        value("input_args", "as in spans"),
        value(
            "network_event_values",
            "as in spans; an unfinished request's events so far",
        ),
        col(
            "context_distinct_id",
            "text",
            "announcement-time identity; NULL when absent or unavailable",
        ).heading("User"),
        value(
            "context_metadata",
            "announcement-time metadata; unavailable without an announcement",
        ),
        col(
            "is_complete",
            "integer",
            "1 once spans has the span's completion",
        ),
    ],
    view: "
CREATE TEMP VIEW span_announcements AS
SELECT __btel_pubid(r.recording_id, c.call_id) AS span_id,
  'function' AS span_type,
  f.fqn AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, c.thread_id) AS future_id,
  __btel_hex(p.node_id) AS profiler_node_id,
  __btel_pubid(r.recording_id, c.parent_id) AS parent_span_id,
  __btel_utc(c.entered_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS start_time,
  __btel_ref(2, c.type_args_cas, NULL,
    c.needs_announcement = 1 AND c.announced_sequence IS NULL) AS type_args,
  __btel_ref(IIF(c.manual = 1 OR f.definition_key LIKE 'host:%', 2, 1), c.inputs_cas, f.argument_names,
    c.needs_announcement = 1 AND c.announced_sequence IS NULL) AS input_args,
  NULL AS network_event_values,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  c.outcome IS NOT NULL AS is_complete,
  c.rec AS __span_id_rec, c.call_id AS __span_id_key,
  c.rec AS __future_id_rec, c.thread_id AS __future_id_key,
  c.rec AS __parent_span_id_rec, c.parent_id AS __parent_span_id_key
FROM main.call c
JOIN main.recording r ON r.rec = c.rec
JOIN main.event_context ec ON ec.rec = c.rec AND ec.node_id = c.call_id AND ec.slot = 1
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path p ON p.rec = c.rec AND p.call_path_id = c.call_path_id
LEFT JOIN main.function_def f ON f.rec = c.rec AND f.function_id = p.callee_function_id
LEFT JOIN main.thread t ON t.rec = c.rec AND t.thread_id = c.thread_id
LEFT JOIN main.epoch e ON e.rec = c.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
UNION ALL
SELECT __btel_pubid(r.recording_id, t.thread_id) AS span_id,
  'future' AS span_type,
  COALESCE(t.name, sf.fqn, (SELECT ef.fqn FROM main.call_path ep
     JOIN main.function_def ef ON ef.rec = ep.rec AND ef.function_id = ep.callee_function_id
     WHERE ep.rec = t.rec AND ep.call_path_id = t.entry_call_path_id)) AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, COALESCE(pt.thread_id, pc.thread_id)) AS future_id,
  __btel_hex(COALESCE(sp.node_id, (SELECT ep.node_id FROM main.call_path ep
     WHERE ep.rec = t.rec AND ep.call_path_id = t.entry_call_path_id))) AS profiler_node_id,
  __btel_pubid(r.recording_id, t.parent_id) AS parent_span_id,
  __btel_utc(COALESCE(t.ran_ticks, t.started_ticks), IIF(e.conflict = 0, e.utc_ticks, NULL),
    e.utc_unix_ns, e.multiplier, e.shift) AS start_time,
  NULL AS type_args,
  NULL AS input_args,
  NULL AS network_event_values,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  t.outcome IS NOT NULL AS is_complete,
  t.rec AS __span_id_rec, t.thread_id AS __span_id_key,
  t.rec AS __future_id_rec, COALESCE(pt.thread_id, pc.thread_id) AS __future_id_key,
  t.rec AS __parent_span_id_rec, t.parent_id AS __parent_span_id_key
FROM main.thread t
JOIN main.recording r ON r.rec = t.rec
JOIN main.event_context ec ON ec.rec = t.rec AND ec.node_id = t.thread_id AND ec.slot = 1
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path sp ON sp.rec = t.rec AND sp.call_path_id = t.spawn_call_path_id
LEFT JOIN main.function_def sf ON sf.rec = t.rec AND sf.function_id = sp.callee_function_id
LEFT JOIN main.thread pt ON pt.rec = t.rec AND pt.thread_id = t.parent_id
LEFT JOIN main.call pc ON pc.rec = t.rec AND pc.call_id = t.parent_id
LEFT JOIN main.epoch e ON e.rec = t.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
WHERE t.defined = 1 OR t.announced = 1 OR t.outcome IS NOT NULL
UNION ALL
SELECT __btel_pubid(r.recording_id, n.span_id) AS span_id,
  'network_span' AS span_type,
  n.method || ' ' || n.url AS span_name,
  lower(hex(COALESCE(r.process_id, r.recording_id))) AS process_id,
  __btel_pubid(r.recording_id, n.thread_id) AS future_id,
  __btel_hex(p.node_id) AS profiler_node_id,
  __btel_pubid(r.recording_id, n.parent_id) AS parent_span_id,
  __btel_utc(n.started_ticks, IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns,
    e.multiplier, e.shift) AS start_time,
  NULL AS type_args,
  __btel_ref(2, n.request_cas, NULL, n.defined = 0) AS input_args,
  (SELECT __btel_events(v.at_ticks, v.seq, v.name, __btel_utc(v.at_ticks,
       IIF(e.conflict = 0, e.utc_ticks, NULL), e.utc_unix_ns, e.multiplier, e.shift),
       v.payload_cas)
   FROM main.network_event v WHERE v.rec = n.rec AND v.span_id = n.span_id) AS network_event_values,
  cx.distinct_id AS context_distinct_id,
  __btel_context_metadata(ec.state, ec.cas, cx.state) AS context_metadata,
  n.outcome IS NOT NULL AS is_complete,
  n.rec AS __span_id_rec, n.span_id AS __span_id_key,
  n.rec AS __future_id_rec, n.thread_id AS __future_id_key,
  n.rec AS __parent_span_id_rec, n.parent_id AS __parent_span_id_key
FROM main.network_span n
JOIN main.recording r ON r.rec = n.rec
JOIN main.event_context ec ON ec.rec = n.rec AND ec.node_id = n.span_id AND ec.slot = 1
LEFT JOIN main.context_snapshot cx ON cx.cas = ec.cas
LEFT JOIN main.call_path p ON p.rec = n.rec AND p.call_path_id = n.call_path_id
LEFT JOIN main.thread t ON t.rec = n.rec AND t.thread_id = n.thread_id
LEFT JOIN main.epoch e ON e.rec = n.rec AND e.epoch_id = t.epoch_id AND e.defined = 1
-- Events alone name a span, but say nothing else about it.
WHERE n.defined = 1 OR n.outcome IS NOT NULL",
};

pub const RELATIONS: &[Relation] = &[PROCESSES, PROFILER, SPANS, SPAN_ANNOUNCEMENTS];

/// Id columns a filter can use indexes for: `(relation, column, prefix)`.
/// The relation's view also selects the id's stored parts as hidden columns
/// `__<column>_rec` and `__<column>_key`, which the SQL translator adds to
/// `=` and `IN` on the id. The prefix is the id's kind, as `__btel_key`
/// takes it (empty for spans).
pub const KEYS: &[(&str, &str, &str)] = &[
    ("spans", "span_id", ""),
    ("spans", "future_id", ""),
    ("spans", "parent_span_id", ""),
    ("span_announcements", "span_id", ""),
    ("span_announcements", "future_id", ""),
    ("span_announcements", "parent_span_id", ""),
];

/// The kind of `relation.column` when it is an indexable id.
pub fn key(relation: &str, column: &str) -> Option<&'static str> {
    KEYS.iter()
        .find(|(r, c, _)| r.eq_ignore_ascii_case(relation) && c.eq_ignore_ascii_case(column))
        .map(|(_, _, prefix)| *prefix)
}

pub fn relation(name: &str) -> Option<&'static Relation> {
    RELATIONS
        .iter()
        .find(|relation| relation.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rusqlite::Connection;

    fn plan(conn: &Connection, sql: &str) -> Vec<(i64, i64, String)> {
        conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// Whether `step` scans what the query builds itself (a co-routine, a
    /// materialized subquery, constant rows such as the network provider
    /// tables): it reads no recording's rows.
    fn scans_built_rows(plan: &[(i64, i64, String)], step: &str) -> bool {
        let Some(scanned) = step.strip_prefix("SCAN ") else {
            return false;
        };
        scanned.ends_with("CONSTANT ROW")
            || scanned.ends_with("CONSTANT ROWS")
            || plan.iter().any(|(_, _, other)| {
                other.strip_prefix("CO-ROUTINE ") == Some(scanned)
                    || other.strip_prefix("MATERIALIZE ") == Some(scanned)
            })
    }

    /// Without statistics, SQLite may look rows up by recording alone inside
    /// a loop, rescanning the recording's history once per row. No relation
    /// may plan that way: every inner lookup narrows past `rec`, and full
    /// scans only drive the outermost loop of a (non-correlated) query.
    fn assert_no_rescan(name: &str, plan: &[(i64, i64, String)]) {
        let detail = |id: i64| {
            plan.iter()
                .find(|(step, _, _)| *step == id)
                .map_or("", |(_, _, detail)| detail.as_str())
        };
        let mut outer_loops = HashSet::new();
        for (_, parent, step) in plan {
            if !(step.starts_with("SCAN ") || step.starts_with("SEARCH "))
                || scans_built_rows(plan, step)
            {
                continue;
            }
            let inner = !outer_loops.insert(*parent) || detail(*parent).starts_with("CORRELATED");
            // A compound view's second branch is a second outer loop.
            let compound =
                detail(*parent).starts_with("COMPOUND") || detail(*parent).starts_with("UNION ALL");
            // json_each walks one row's command arguments.
            let rescans = step.ends_with("(rec=?)")
                || (inner
                    && !compound
                    && step.starts_with("SCAN ")
                    && !step.contains("VIRTUAL TABLE"));
            assert!(!rescans, "{name} rescans per row: {step}\n{plan:#?}");
        }
    }

    fn connection() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::store::ensure_schema(&mut conn).unwrap();
        crate::functions::register(&conn, &crate::functions::ContextSlot::default()).unwrap();
        for relation in super::RELATIONS {
            conn.execute_batch(&relation.create_sql()).unwrap();
        }
        conn
    }

    #[test]
    fn relations_never_rescan_a_recording_per_row() {
        let conn = connection();
        for relation in super::RELATIONS {
            let sql = format!("SELECT * FROM {}", relation.name);
            assert_no_rescan(relation.name, &plan(&conn, &sql));
        }
        // Keyed filters narrow to one row per branch, on an index.
        for (relation, column, _) in super::KEYS {
            let sql = format!(
                "SELECT * FROM {relation} WHERE __{column}_rec = 1 AND __{column}_key = x'0000000000000001'"
            );
            let plan = plan(&conn, &sql);
            // A future's own future_id is derived from its parent: that
            // branch reads the recording's futures once.
            if *column != "future_id" {
                assert_no_rescan(relation, &plan);
                assert!(
                    plan.iter()
                        .filter(|(_, _, step)| step.starts_with("SCAN "))
                        .all(|(_, _, step)| {
                            // Only the view's own rows, already narrowed.
                            step == &format!("SCAN {relation}")
                                || step.contains("VIRTUAL TABLE")
                                || scans_built_rows(&plan, step)
                        }),
                    "{relation}.{column} scans:\n{plan:#?}"
                );
            }
        }
    }

    #[test]
    fn profiler_rows_carry_their_process_times() {
        let conn = connection();
        conn.execute_batch(
            "INSERT INTO recording (rec, recording_id, process_id, process_started_ns,
               process_end_status, process_end_ns, updated_ns) VALUES
               -- Two engines of one process: the earliest start, the known end.
               (1, x'01', x'a1', 2000000000000, 1, 7000000000000, 9000000000000),
               (2, x'02', x'a1', 1000000000000, 4, 8000000000000, 9500000000000),
               -- No reported end: the newest file indexed.
               (3, x'03', x'a2', 3000000000000, NULL, NULL, 6000000000000),
               -- Neither: its start.
               (4, x'04', x'a3', 4000000000000, NULL, NULL, NULL),
               -- A recording without a process is its own, with no start.
               (5, x'05', NULL, NULL, 2, 5000000000000, 5500000000000);
             INSERT INTO profile_node (process_id, node_id, node_type) VALUES
               (x'a1', 1, 'function'), (x'a1', 2, 'future'), (x'a2', 1, 'function'),
               (x'a3', 1, 'function'), (x'05', 1, 'function');",
        )
        .unwrap();
        // Seconds since the epoch, as the relation renders a timestamp.
        let utc = |seconds: Option<i64>| -> Option<String> {
            conn.query_row(
                "SELECT __btel_ns_utc(?1)",
                [seconds.map(|s| s * 1_000_000_000)],
                |r| r.get(0),
            )
            .unwrap()
        };
        let rows: Vec<(String, Option<String>, Option<String>)> = conn
            .prepare(
                "SELECT process_id, process_start_time, process_end_time FROM profiler
                 ORDER BY process_id, profiler_node_id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let row = |process: &str, start: Option<i64>, end: i64| {
            (process.to_owned(), utc(start), utc(Some(end)))
        };
        assert_eq!(
            rows,
            vec![
                row("05", None, 5000),
                row("a1", Some(1000), 7000),
                row("a1", Some(1000), 7000),
                row("a2", Some(3000), 6000),
                row("a3", Some(4000), 4000),
            ]
        );
        assert_ne!(utc(Some(7000)), utc(Some(6000)));
        // The times agree with `processes`, and bound rows without a join.
        let disagreeing: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM profiler n JOIN processes p ON p.process_id = n.process_id
                 WHERE n.process_end_time IS NOT p.last_updated AND p.process_id <> 'a3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(disagreeing, 0);
        let bounded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM profiler WHERE process_end_time >= __btel_ns_utc(6000000000000)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bounded, 3);
    }

    /// Every profiler row of a process carries the source revision the
    /// process ran, as `processes` reports it: NULL for a process that
    /// recorded no sources.
    #[test]
    fn profiler_rows_carry_their_process_source_revision() {
        let conn = connection();
        conn.execute_batch(
            "INSERT INTO recording (rec, recording_id, process_id, source_cas) VALUES
               -- Two engines of one process, one of which recorded no sources.
               (1, x'01', x'a1', x'AB01'), (2, x'02', x'a1', NULL),
               -- Another revision.
               (3, x'03', x'a2', x'cd02'),
               -- No sources recorded.
               (4, x'04', x'a3', NULL),
               -- A recording without a process is its own.
               (5, x'05', NULL, x'ab01');
             INSERT INTO profile_node (process_id, node_id, node_type) VALUES
               (x'a1', 1, 'function'), (x'a1', 2, 'future'), (x'a2', 1, 'function'),
               (x'a3', 1, 'function'), (x'05', 1, 'function');",
        )
        .unwrap();
        let rows: Vec<(String, Option<String>)> = conn
            .prepare(
                "SELECT process_id, baml_source_code_content_id FROM profiler
                 ORDER BY process_id, profiler_node_id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let row = |process: &str, revision: Option<&str>| {
            (process.to_owned(), revision.map(str::to_owned))
        };
        assert_eq!(
            rows,
            vec![
                row("05", Some("ab01")),
                row("a1", Some("ab01")),
                row("a1", Some("ab01")),
                row("a2", Some("cd02")),
                row("a3", None),
            ]
        );
        // The revision agrees with `processes`, NULL included.
        let (joined, disagreeing): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),
                   SUM(n.baml_source_code_content_id IS NOT p.baml_source_code_content_id)
                 FROM profiler n JOIN processes p ON p.process_id = n.process_id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((joined, disagreeing), (5, 0));
        // It picks one revision's code paths without a join, and an absent
        // revision is NULL, not the empty string.
        let count = |filter: &str| -> i64 {
            conn.query_row(
                &format!("SELECT COUNT(*) FROM profiler WHERE {filter}"),
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(count("baml_source_code_content_id = 'ab01'"), 3);
        assert_eq!(count("baml_source_code_content_id IS NULL"), 1);
        assert_eq!(count("baml_source_code_content_id = ''"), 0);
    }

    /// A column's labels are for the values its doc names, so neither goes stale alone, and
    /// they reach the JSON schema with the column.
    #[test]
    fn labeled_values_are_the_documented_ones() {
        let mut labeled = Vec::new();
        for relation in super::RELATIONS {
            for column in relation.columns {
                for label in column.labels {
                    assert!(
                        !label.label.is_empty() && !label.meaning.is_empty(),
                        "{}.{} {}",
                        relation.name,
                        column.name,
                        label.value
                    );
                    // Liveness statuses are the cloud's, and the doc is of the shared ones.
                    assert!(
                        column.doc.contains(label.value)
                            || label.meaning.starts_with("BAML Cloud only"),
                        "{}.{} documents no `{}`",
                        relation.name,
                        column.name,
                        label.value
                    );
                }
                if !column.labels.is_empty() {
                    labeled.push(format!("{}.{}", relation.name, column.name));
                }
            }
        }
        assert_eq!(
            labeled,
            [
                "processes.status",
                "profiler.node_type",
                "spans.span_type",
                "spans.status",
                "span_announcements.span_type",
            ]
        );

        let spans = serde_json::to_value(super::SPANS).unwrap();
        let column = |name: &str| {
            spans["columns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|column| column["name"] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(
            column("status")["labels"][1],
            serde_json::json!({
                "value": "user_error",
                "label": "Failed",
                "meaning": "the call ended with an error the program threw; not a mistake of the program's user",
            })
        );
        assert_eq!(column("context_distinct_id")["heading"], "User");
        // A column that reads as it is carries neither.
        assert!(column("duration").get("heading").is_none());
        assert!(column("duration").get("labels").is_none());
    }

    /// Every relation's documented columns are exactly its view's columns.
    #[test]
    fn documented_columns_match_views() {
        let conn = connection();
        for relation in super::RELATIONS {
            let statement = conn
                .prepare(&format!("SELECT * FROM {}", relation.name))
                .unwrap();
            let names = statement.column_names();
            let (hidden, actual): (Vec<&str>, Vec<&str>) =
                names.iter().partition(|name| name.starts_with("__"));
            let documented: Vec<&str> = relation.columns.iter().map(|c| c.name).collect();
            assert_eq!(actual, documented, "{}", relation.name);
            // Hidden columns are exactly the stored parts of its keyed ids.
            let keyed: Vec<String> = super::KEYS
                .iter()
                .filter(|(r, _, _)| *r == relation.name)
                .flat_map(|(_, c, _)| [format!("__{c}_rec"), format!("__{c}_key")])
                .collect();
            assert_eq!(hidden, keyed, "{}", relation.name);
            for (_, column, _) in super::KEYS.iter().filter(|(r, _, _)| *r == relation.name) {
                assert!(documented.contains(column), "{}.{column}", relation.name);
            }
        }
    }
}
