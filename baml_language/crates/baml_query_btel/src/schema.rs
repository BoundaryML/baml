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
//! - A reference to a thread, call path, function or clock epoch whose
//!   definition is not indexed yet gets a placeholder row (`defined = 0`, or
//!   `function_def.state = 0`). Placeholders keep unresolved identities
//!   visible; they never make a thread a root or a path a top-level path.

/// Physical layout of these tables. Change on any DDL change.
pub const SCHEMA_VERSION: i64 = 7;
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
  generation INTEGER NOT NULL DEFAULT 0
) STRICT;

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
  -- Distance from a top-level path (0), once every ancestor is defined.
  depth INTEGER,
  -- Sum of synchronous children's normal-node duration totals. Reduced
  -- when an affected child's evidence changes, in the same transaction.
  -- Reentry and spawn time are excluded. NULL means overflow; nanoseconds
  -- and clock validity are still evaluated at query time.
  direct_child_ticks INTEGER DEFAULT 0,
  -- The call or spawn expression at caller_pc, resolved in the caller's
  -- recorded source map (btel_reader::source_map::SiteState labels). NULL
  -- until the path is defined; re-resolved when the caller's definition
  -- arrives later.
  site_state TEXT,
  site_file TEXT,
  site_line INTEGER,
  site_start INTEGER,
  site_end INTEGER,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, call_path_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX call_path_by_caller ON call_path (rec, caller_function_id);
-- Covers the executions view's entry-function lookup: without statistics,
-- SQLite otherwise prefers scanning the recording's primary-key range.
CREATE INDEX call_path_by_thread
  ON call_path (rec, thread_id, parent_call_path_id, edge, callee_function_id);
CREATE INDEX call_path_by_parent ON call_path (rec, parent_call_path_id, edge);
-- Paths still waiting for a depth: resolution touches only these.
CREATE INDEX call_path_undepthed ON call_path (rec) WHERE depth IS NULL;

CREATE TABLE thread (
  rec INTEGER NOT NULL,
  thread_id BLOB NOT NULL,
  defined INTEGER NOT NULL,
  parent_id BLOB,
  spawn_call_path_id INTEGER,
  started_ticks INTEGER,
  epoch_id BLOB,
  announced INTEGER NOT NULL DEFAULT 0,
  completed_ticks INTEGER,
  outcome INTEGER,
  -- A second completion disagreed with the first (which is kept).
  completion_conflict INTEGER NOT NULL DEFAULT 0,
  -- The execution (root thread) this thread belongs to, once resolvable.
  root_id BLOB,
  conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, thread_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX thread_by_root ON thread (rec, root_id);
-- Threads still waiting for their execution: root resolution after each
-- transaction touches only these, not the recording's history.
CREATE INDEX thread_unresolved ON thread (rec) WHERE root_id IS NULL;

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
  PRIMARY KEY (rec, node)
) STRICT, WITHOUT ROWID;

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
  completed_sequence INTEGER,
  late INTEGER,
  exited_ticks INTEGER,
  self_await_ticks INTEGER,
  outcome INTEGER,
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

-- One observed start of unwinding (a raise), merged with its end by ID.
-- Enum columns hold the wire enum values. defined = 0 is an end whose raise
-- is not indexed. Sites use the same states as call_path.site_state.
CREATE TABLE error_raise (
  rec INTEGER NOT NULL,
  raise_id BLOB NOT NULL,
  defined INTEGER NOT NULL,
  sequence INTEGER,
  thread_id BLOB,
  raised_ticks INTEGER,
  kind INTEGER,
  function_id BLOB,
  pc INTEGER,
  origin_state INTEGER,
  origin_raise_id BLOB,
  previous_raise_id BLOB,
  origin_via INTEGER,
  origin_candidates INTEGER,
  unresolved_reason INTEGER,
  frame_count INTEGER,
  -- The raising frame's call path: the stack is function_id at pc, then
  -- the callers raise_path_frame lists. NULL when error_frame lists it.
  call_path_id INTEGER,
  inherited_count INTEGER,
  -- JSON array of {function, file, line}; NULL when none was recorded.
  inherited TEXT,
  -- Encoded raise, compared to detect conflicting duplicates.
  evidence BLOB,
  site_state TEXT,
  site_file TEXT,
  site_line INTEGER,
  site_start INTEGER,
  site_end INTEGER,
  end_sequence INTEGER,
  end_result INTEGER,
  handler_function_id BLOB,
  handler_pc INTEGER,
  unwound_frames INTEGER,
  handler_site_state TEXT,
  handler_site_file TEXT,
  handler_site_line INTEGER,
  handler_site_start INTEGER,
  handler_site_end INTEGER,
  conflict INTEGER NOT NULL DEFAULT 0,
  end_conflict INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (rec, raise_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX error_raise_by_origin ON error_raise (rec, origin_raise_id);
CREATE INDEX error_raise_by_function ON error_raise (rec, function_id);
CREATE INDEX error_raise_by_handler ON error_raise (rec, handler_function_id);
CREATE INDEX error_raise_by_thread ON error_raise (rec, thread_id);

-- Call paths that name raise stacks (error_raise.call_path_id). built = 1
-- once raise_path_frame lists the path's callers; 0 while a path it walks
-- through is not defined yet. One row per path, not per raise.
CREATE TABLE raise_path (
  rec INTEGER NOT NULL,
  call_path_id INTEGER NOT NULL,
  built INTEGER NOT NULL,
  PRIMARY KEY (rec, call_path_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX raise_path_unbuilt ON raise_path (rec) WHERE built = 0;

-- The callers on a raise path, innermost first. Position 1 is the raising
-- frame's caller: frame_path_id's caller_function_id at its caller_pc, with
-- that path's call site. Ends at the thread's first frame, at most 63 rows.
CREATE TABLE raise_path_frame (
  rec INTEGER NOT NULL,
  call_path_id INTEGER NOT NULL,
  position INTEGER NOT NULL,
  frame_path_id INTEGER NOT NULL,
  PRIMARY KEY (rec, call_path_id, position)
) STRICT, WITHOUT ROWID;

-- A raise's live stack, innermost first (position 0), when it was recorded
-- explicitly because no call path covered it.
CREATE TABLE error_frame (
  rec INTEGER NOT NULL,
  raise_id BLOB NOT NULL,
  position INTEGER NOT NULL,
  function_id BLOB,
  pc INTEGER,
  native INTEGER NOT NULL,
  site_state TEXT,
  site_file TEXT,
  site_line INTEGER,
  site_start INTEGER,
  site_end INTEGER,
  PRIMARY KEY (rec, raise_id, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX error_frame_by_function ON error_frame (rec, function_id);

-- Retained calls a raise names: role 1 its raise frame, 2 a call it unwound.
CREATE TABLE error_link (
  rec INTEGER NOT NULL,
  raise_id BLOB NOT NULL,
  call_id BLOB NOT NULL,
  role INTEGER NOT NULL,
  sequence INTEGER NOT NULL,
  PRIMARY KEY (rec, raise_id, call_id, role)
) STRICT, WITHOUT ROWID;
-- A call completes once, so one raise can have unwound it.
CREATE UNIQUE INDEX error_link_unwound ON error_link (rec, call_id) WHERE role = 2;
CREATE INDEX error_link_by_call ON error_link (rec, call_id, role);

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
    "ledger",
    "rejected",
    "function_def",
    "function_param",
    "call_path",
    "thread",
    "epoch",
    "epoch_state",
    "aggregate",
    "call",
    "error_raise",
    "raise_path",
    "raise_path_frame",
    "error_frame",
    "error_link",
    "issue",
];

pub const ALL_TABLES: &[&str] = &[
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
    "call",
    "error_raise",
    "raise_path",
    "raise_path_frame",
    "error_frame",
    "error_link",
    "issue",
];
