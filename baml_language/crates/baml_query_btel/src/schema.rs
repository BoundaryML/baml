//! Internal tables of the derived index. Disposable: a version mismatch
//! rebuilds everything from the recording files.
//!
//! Representation rules:
//! - Wire identities (thread, call, function, epoch and domain IDs) are u64
//!   and stored as 8-byte big-endian BLOBs: lossless, ordered like the
//!   unsigned value, never interpreted as signed integers.
//! - Quantities (ticks, counts, tick totals) are INTEGER only when they fit
//!   i64; otherwise NULL, with an issue row or overflow meaning explicit.
//!   Reduced aggregate totals also keep an exact decimal copy.
//! - `rec` is a surrogate key local to this database file.
//! - A reference to a thread, call path, function, clock epoch or network
//!   span whose definition is not indexed yet gets a placeholder row
//!   (`defined = 0`, or `function_def.state = 0`). Placeholders keep unresolved identities
//!   visible; they never make a thread a root or a path a top-level path.

/// Physical layout of these tables. Change on any DDL change.
pub const SCHEMA_VERSION: i64 = 16;
/// Interpretation of evidence into rows. Change when reconciliation changes
/// meaning without a DDL change; either mismatch rebuilds the index.
pub const NORMALIZATION_VERSION: i64 = 1;

pub const DDL: &str = r"
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value ANY
) STRICT;

CREATE TABLE recording (
  rec INTEGER PRIMARY KEY,
  recording_id BLOB NOT NULL UNIQUE,
  source_snapshot_id BLOB,
  format_minor INTEGER NOT NULL DEFAULT 0,
  -- Contiguous applied prefix; the watermark queries answer from.
  indexed_sequence INTEGER NOT NULL DEFAULT 0,
  -- Highest completed file seen by the latest discovery.
  observed_sequence INTEGER NOT NULL DEFAULT 0,
  -- Sequence carrying RecordingEnd, once applied.
  terminal_sequence INTEGER,
  -- First sequence that could not be applied, and why ('missing'/'invalid').
  blocked_sequence INTEGER,
  blocked_reason TEXT,
  partial_files INTEGER NOT NULL DEFAULT 0,
  files_after_end INTEGER NOT NULL DEFAULT 0,
  indexed_bytes INTEGER NOT NULL DEFAULT 0,
  generation INTEGER NOT NULL DEFAULT 0,
  -- The process that ran this engine, from the first header that names it
  -- (format minor 3). Engines of one process share process_id; recordings
  -- without one are their own process.
  process_id BLOB,
  baml_version TEXT,
  host TEXT,
  -- JSON array of the process's arguments.
  command TEXT,
  process_started_ns INTEGER,
  -- CAS id of the project's sources, a map<path, content> snapshot.
  source_cas BLOB,
  -- How the process ended, when this recording's end marker says so:
  -- 1 success, 2 error, 3 panicked.
  process_end_status INTEGER,
  process_end_ns INTEGER,
  -- Newest modification time of an applied file, Unix nanoseconds.
  updated_ns INTEGER,
  -- 1 once its end is indexed: its paths are summed in profile_part, and
  -- only the paths spans read remain, with no aggregate or sysop rows.
  folded INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX recording_by_process ON recording (process_id);

-- Identity is resolved once per snapshot. State 0 retries changed files on refresh,
-- 1 is resolved (including a null identity), 2 is invalid context data.
CREATE TABLE context_snapshot (
  cas BLOB PRIMARY KEY,
  state INTEGER NOT NULL DEFAULT 0,
  distinct_id TEXT,
  failed_stamp BLOB
) STRICT, WITHOUT ROWID;
CREATE INDEX context_by_identity ON context_snapshot (distinct_id);
CREATE INDEX context_pending ON context_snapshot (state) WHERE state = 0;

-- Slot 0 is entry-time span context; slot 1 is announcement-time context.
-- State: 0 unavailable, 1 explicitly empty, 2 snapshot, 3 invalid marker.
CREATE TABLE event_context (
  rec INTEGER NOT NULL,
  node_id BLOB NOT NULL,
  slot INTEGER NOT NULL,
  priority INTEGER NOT NULL,
  state INTEGER NOT NULL,
  cas BLOB,
  PRIMARY KEY (rec, node_id, slot)
) STRICT, WITHOUT ROWID;
CREATE INDEX event_context_by_cas ON event_context (cas);

-- One row per applied file. Facts, totals and this ledger commit together.
CREATE TABLE ledger (
  rec INTEGER NOT NULL,
  sequence INTEGER NOT NULL,
  size INTEGER NOT NULL,
  modified_ns INTEGER,
  device BLOB NOT NULL,
  inode BLOB NOT NULL,
  content_hash BLOB NOT NULL,
  PRIMARY KEY (rec, sequence)
) STRICT, WITHOUT ROWID;

-- The unapplicable file at a recording's frontier, so an unchanged invalid
-- file is not decoded again on every refresh.
CREATE TABLE rejected (
  rec INTEGER NOT NULL,
  sequence INTEGER NOT NULL,
  size INTEGER NOT NULL,
  modified_ns INTEGER,
  device BLOB NOT NULL,
  inode BLOB NOT NULL,
  reason TEXT NOT NULL,
  PRIMARY KEY (rec, sequence)
) STRICT, WITHOUT ROWID;

CREATE TABLE function_def (
  rec INTEGER NOT NULL,
  function_id BLOB NOT NULL,
  -- 0 referenced only, 1 lookup-unavailable observed, 2 metadata recorded.
  state INTEGER NOT NULL,
  fqn TEXT,
  display_name TEXT,
  definition_key TEXT,
  kind TEXT,
  kind_detail TEXT,
  origin TEXT,
  source_file TEXT,
  source_file_id INTEGER,
  source_start INTEGER,
  source_end INTEGER,
  package TEXT,
  -- Namespace components joined with '.', and as a JSON array.
  namespace TEXT,
  namespace_json TEXT,
  owner_type_key TEXT,
  parent_function_key TEXT,
  lambda_path TEXT,
  -- btel_reader::evidence::ArgumentNames encoding; NULL = names unknown.
  argument_names BLOB,
  -- Slot count of a recorded layout; NULL = layout unknown.
  parameter_count INTEGER,
  -- Encoded wire metadata, compared to detect conflicting definitions.
  metadata BLOB,
  -- btel_reader::source_map::SourceMap encoding of the recorded line table.
  -- source_map_state: NULL none recorded, 1 valid, 2 rejected as invalid.
  source_map BLOB,
  source_map_state INTEGER,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, function_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX function_def_unresolved ON function_def (rec) WHERE state < 2;

-- Recorded parameter slots; rows exist only for a known layout.
CREATE TABLE function_param (
  rec INTEGER NOT NULL,
  function_id BLOB NOT NULL,
  position INTEGER NOT NULL,
  name TEXT,
  receiver INTEGER NOT NULL,
  PRIMARY KEY (rec, function_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE call_path (
  rec INTEGER NOT NULL,
  call_path_id INTEGER NOT NULL,
  defined INTEGER NOT NULL,
  thread_id BLOB,
  parent_call_path_id INTEGER,
  caller_function_id BLOB,
  caller_pc INTEGER,
  callee_function_id BLOB,
  edge INTEGER,
  -- The profiler node: a hash of the function names from the top-level
  -- path down, with each edge kind. NULL until every ancestor's function is
  -- known; cleared and recomputed when one of them changes.
  node_id INTEGER,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, call_path_id)
) STRICT, WITHOUT ROWID;
-- Paths still waiting for a profiler node.
CREATE INDEX call_path_unnoded ON call_path (rec) WHERE node_id IS NULL;

CREATE TABLE thread (
  rec INTEGER NOT NULL,
  thread_id BLOB NOT NULL,
  defined INTEGER NOT NULL,
  parent_id BLOB,
  spawn_call_path_id INTEGER,
  -- When it was created: for a spawned future, when it was scheduled.
  started_ticks INTEGER,
  epoch_id BLOB,
  announced INTEGER NOT NULL DEFAULT 0,
  -- When a spawned future's body began running (or it was cancelled before
  -- it ran); NULL for a root thread and for recordings before format minor 6.
  ran_ticks INTEGER,
  completed_ticks INTEGER,
  outcome INTEGER,
  -- 1 when a panic ended it (outcome is errored).
  panicked INTEGER NOT NULL DEFAULT 0,
  -- The future's name, when the spawn gave it one.
  name TEXT,
  -- A second completion disagreed with the first (which is kept).
  completion_conflict INTEGER NOT NULL DEFAULT 0,
  conflict INTEGER NOT NULL DEFAULT 0,
  -- The thread's first top-level synchronous path: the function a root
  -- future ran, which names it when nothing else does.
  entry_call_path_id INTEGER,
  PRIMARY KEY (rec, thread_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX thread_by_parent ON thread (rec, parent_id);

CREATE TABLE epoch (
  rec INTEGER NOT NULL,
  epoch_id BLOB NOT NULL,
  defined INTEGER NOT NULL,
  domain_id BLOB,
  source INTEGER,
  multiplier INTEGER,
  shift INTEGER,
  utc_ticks INTEGER,
  utc_unix_ns INTEGER,
  -- Encoded wire definition: the clocks relation reads every field from it.
  definition BLOB,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, epoch_id)
) STRICT, WITHOUT ROWID;

-- Most severe observed validity; states can arrive before or after the
-- definition they describe.
CREATE TABLE epoch_state (
  rec INTEGER NOT NULL,
  epoch_id BLOB NOT NULL,
  status INTEGER NOT NULL,
  final INTEGER NOT NULL,
  PRIMARY KEY (rec, epoch_id)
) STRICT, WITHOUT ROWID;

-- Reduced aggregate totals per call-path node (path << 1 | reentry).
-- INTEGER totals are NULL once they no longer fit i64; the *_exact
-- decimals keep the exact sum of every applied delta.
CREATE TABLE aggregate (
  rec INTEGER NOT NULL,
  node INTEGER NOT NULL,
  call_count INTEGER,
  duration_ticks INTEGER,
  self_await_ticks INTEGER,
  count_exact TEXT NOT NULL,
  duration_exact TEXT NOT NULL,
  self_await_exact TEXT NOT NULL,
  -- Evidence bits: 1 recorded, 2 missing in an old delta, 4 invalid.
  -- Mixed evidence stays unknown; never assume missing means success.
  outcome_evidence INTEGER NOT NULL,
  ok_calls INTEGER,
  errored_calls INTEGER,
  cancelled_calls INTEGER,
  errored_exact TEXT NOT NULL,
  cancelled_exact TEXT NOT NULL,
  -- The errored calls a panic ended; NULL unless every delta counted them.
  panicked_calls INTEGER,
  panicked_exact TEXT NOT NULL DEFAULT '0',
  PRIMARY KEY (rec, node)
) STRICT, WITHOUT ROWID;

-- Time spent in sysops per call path; total_ticks NULL once it overflows.
CREATE TABLE sysop (
  rec INTEGER NOT NULL,
  call_path_id INTEGER NOT NULL,
  sysops INTEGER NOT NULL,
  total_ticks INTEGER,
  PRIMARY KEY (rec, call_path_id)
) STRICT, WITHOUT ROWID;

-- Tokens each model turn used, in file order; several rows can name one node.
CREATE TABLE model_usage (
  rec INTEGER NOT NULL,
  sequence INTEGER NOT NULL,
  position INTEGER NOT NULL,
  node_id BLOB NOT NULL,
  model TEXT,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER,
  cache_write_tokens INTEGER,
  reasoning_tokens INTEGER,
  PRIMARY KEY (rec, sequence, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX model_usage_by_node ON model_usage (rec, node_id);

-- Retained calls, merged from announcements and completions by identity.
CREATE TABLE call (
  rec INTEGER NOT NULL,
  call_id BLOB NOT NULL,
  thread_id BLOB NOT NULL,
  parent_id BLOB NOT NULL,
  call_path_id INTEGER NOT NULL,
  reentry INTEGER,
  announced_sequence INTEGER,
  entered_ticks INTEGER,
  inputs_cas BLOB,
  -- A generic call's type arguments, map<string, Type> (format minor 8).
  type_args_cas BLOB,
  completed_sequence INTEGER,
  late INTEGER,
  exited_ticks INTEGER,
  self_await_ticks INTEGER,
  outcome INTEGER,
  -- 1 when a panic ended it (outcome is errored).
  panicked INTEGER,
  needs_announcement INTEGER,
  value_cas BLOB,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, call_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX call_by_thread ON call (rec, thread_id);
CREATE INDEX call_by_parent ON call (rec, parent_id);
-- Conflicts not yet reported as issues; normally empty.
CREATE INDEX call_unreported_conflict ON call (rec) WHERE conflict = 1;
-- Completions still waiting for their input announcement; normally empty.
CREATE INDEX call_pending ON call (rec)
  WHERE needs_announcement = 1 AND announced_sequence IS NULL;

-- HTTP requests the runtime traced, merged from announcements, events and
-- completions by span ID. Events and completions can arrive before the
-- announcement, and in another thread's section; until it arrives the row
-- is a placeholder (defined = 0).
CREATE TABLE network_span (
  rec INTEGER NOT NULL,
  span_id BLOB NOT NULL,
  defined INTEGER NOT NULL,
  -- The announcing section's thread: its epoch times every tick of the span.
  thread_id BLOB,
  parent_id BLOB,
  -- The frame that made the request; NULL outside any.
  call_path_id INTEGER,
  method TEXT,
  -- Sanitized when recorded: query values and credentials hashed.
  url TEXT,
  request_cas BLOB,
  started_ticks INTEGER,
  completed_ticks INTEGER,
  outcome INTEGER,
  -- 1 when a panic ended it (outcome is errored).
  panicked INTEGER NOT NULL DEFAULT 0,
  error_cas BLOB,
  -- Two announcements or two completions disagreed; the first is kept.
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, span_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX network_span_by_thread ON network_span (rec, thread_id);
CREATE INDEX network_span_by_parent ON network_span (rec, parent_id);

-- A network span's events: connection, data, end, await, close, drop, or
-- another protocol's names.
CREATE TABLE network_event (
  rec INTEGER NOT NULL,
  span_id BLOB NOT NULL,
  -- Recording order: the file's sequence << 32, plus the event's position
  -- among the file's network events.
  seq INTEGER NOT NULL,
  name TEXT NOT NULL,
  at_ticks INTEGER,
  payload_cas BLOB,
  PRIMARY KEY (rec, span_id, seq)
) STRICT, WITHOUT ROWID;

-- A folded recording's call paths summed by profiler node: its share of its
-- process's profiler. Sums are NULL when unknown or too large for i64.
CREATE TABLE profile_part (
  rec INTEGER NOT NULL,
  node_id INTEGER NOT NULL,
  parent_node_id INTEGER,
  synchronous INTEGER NOT NULL,
  function_name TEXT,
  total_ns INTEGER,
  io_self_ns INTEGER,
  invocation_count INTEGER,
  errored_count INTEGER,
  cancelled_count INTEGER,
  panicked_count INTEGER,
  PRIMARY KEY (rec, node_id)
) STRICT, WITHOUT ROWID;

-- The profiler of each ended process: one row per calling-context node,
-- merged over every recording of the process in this index. Rebuilt when a
-- process's recordings change after its end was indexed.
CREATE TABLE profile_node (
  process_id BLOB NOT NULL,
  node_id INTEGER NOT NULL,
  parent_node_id INTEGER,
  -- 'future' for a spawned future's node (its spawn edge), else 'function'.
  node_type TEXT NOT NULL,
  function_name TEXT,
  total_ns INTEGER,
  self_ns INTEGER,
  io_total_ns INTEGER,
  io_self_ns INTEGER,
  invocation_count INTEGER,
  return_count INTEGER,
  future_cancel_count INTEGER,
  error_count INTEGER,
  panic_error_count INTEGER,
  nonpanic_error_count INTEGER,
  missing_count INTEGER,
  PRIMARY KEY (process_id, node_id)
) STRICT, WITHOUT ROWID;
-- Processes whose profiler must be rebuilt: added in the transaction that
-- changes or removes one of their recordings, removed in the rebuild's own
-- transaction, so a crash between the two leaves the work for next time.
CREATE TABLE profile_pending (
  process_id BLOB PRIMARY KEY
) STRICT, WITHOUT ROWID;

-- Evidence problems found while applying files. State-dependent problems
-- (gaps, unresolved references) are derived at query time instead.
CREATE TABLE issue (
  issue_id INTEGER PRIMARY KEY,
  rec INTEGER NOT NULL,
  sequence INTEGER,
  code TEXT NOT NULL,
  subject TEXT,
  detail TEXT NOT NULL
) STRICT;
CREATE INDEX issue_by_rec ON issue (rec);
";

/// Every fact table keyed by `rec`, for per-recording rebuilds.
pub const FACT_TABLES: &[&str] = &[
    "event_context",
    "ledger",
    "rejected",
    "function_def",
    "function_param",
    "call_path",
    "thread",
    "epoch",
    "epoch_state",
    "aggregate",
    "sysop",
    "model_usage",
    "call",
    "network_span",
    "network_event",
    "profile_part",
    "issue",
];

pub const ALL_TABLES: &[&str] = &[
    "context_snapshot",
    "event_context",
    "meta",
    "recording",
    "ledger",
    "rejected",
    "function_def",
    "function_param",
    "call_path",
    "thread",
    "epoch",
    "epoch_state",
    "aggregate",
    "sysop",
    "model_usage",
    "call",
    "network_span",
    "network_event",
    "profile_part",
    "profile_node",
    "profile_pending",
    "issue",
];
