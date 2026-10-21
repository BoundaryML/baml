# `baml query` — The Tables (catalog v1)

A plain-language guide to what `baml query` lets you query, what each table
means, and where its data comes from. The engineering specs are
`TASK/baml-query-scope.md` (query layer) and
`TASK/profiling-backend-streams.md` (the storage it reads). This document
restates their catalog for readers who want to *use* it, not build it.

---

## 1. What it is

Every time a BAML function runs — from `baml run`, `baml test`, the
playground, or a host program — the profiler records what happened into
`.baml/profiles-v1/`. `baml query` is a SQL prompt over those recordings:

~~~text
baml query "SELECT thread_id, status, total_errors, started_at FROM threads WHERE parent_thread_id IS NULL ORDER BY started_at DESC LIMIT 20"
baml query "SELECT fqn, sum(self_ns) FROM call_path_stats GROUP BY fqn ORDER BY 2 DESC LIMIT 10"
baml query "SELECT call_id, args['customer']['age'] FROM calls WHERE args['customer']['age'] >= 30"
baml query --schema          # prints everything below as JSON
~~~

It is DataFusion SQL (PostgreSQL-flavoured): joins, `GROUP BY`, CTEs, window
functions all work. Every result ends with an **outcome** (complete /
incomplete / budget / cancelled, plus how many captured values could and
could not be evaluated), so you always know whether the answer is whole.

### The three ideas you need

1. **An execution is a thread with no parent.** When a host calls a BAML
   function, the engine starts a fresh logical thread for it. That thread,
   plus every thread it spawns, is one *execution*. There is no separate
   "run" object — not even a view: an execution's row is its root thread,
   selected with `threads WHERE parent_thread_id IS NULL`, and its id is the
   root thread's id (`baml_thread_1_…`).
2. **Population vs. retained.** The profiler counts *every* call into a
   calling-context tree (`call_path_stats`: "this function, called from this call
   site, under this parent — N times, this much time"), but keeps an exact
   record (`calls`) only for *selected* calls: the execution root, every LLM
   call, and calls the program marked with `$id`. So `call_path_stats` answers "how
   much / how often", `calls` answers "show me this one".
3. **Values are captured, not copied into rows.** Inputs/outputs/errors of
   retained calls live in a content-addressed store. In SQL they appear as
   virtual columns (`args`, `output`, `error`) you can subscript and compare;
   they are loaded only when a query touches them. Their identity (`args_cid`)
   is a plain column you can join on without loading anything.

### Identity scheme

| thing | id column(s) | looks like |
|---|---|---|
| execution | `execution_id` (= the root thread's `thread_id`) | `baml_thread_1_AQ…` (58 chars) |
| thread | `thread_id` | `baml_thread_1_…` |
| call | `call_id` | `baml_call_1_…` (67 chars) |
| calling context (CCT node) | `call_path_id` | 64-hex-char hash |
| the program's runtime id for the root (`baml.id.current()` at the root) | `runtime_id` | `baml_id_1_…` (32 chars) |
| captured value | `*_cid` | `bamlv_1_…` + sha256 hex |
| function (within one compiled program) | `(program_id, function_id)`; `fqn`/`definition_key` across programs | `MyPkg.ScoreCustomer` |

Times: `*_ns` columns are nanoseconds relative to the process start (exact,
always present); `*_at` columns are wall-clock timestamps derived from them.

---

## 2. The tables

Every table name is versioned (`threads_v1`) with an unversioned alias
(`threads`). `?` marks a nullable column. Types are Arrow/DataFusion types.

### `threads` — one row per logical thread (root rows are executions)

| column | type | meaning |
|---|---|---|
| execution_id | Utf8 | root thread of this thread's execution |
| thread_id | Utf8 | this thread |
| parent_thread_id? | Utf8 | NULL ⇒ **this is an execution** |
| spawn_call_id?, spawn_function_id?, spawn_fqn? | Utf8 / UInt32 / Utf8 | the call that spawned this thread |
| spawn_site_file?, spawn_site_line? | Utf8 / UInt32 | where the spawn expression is |
| name? | Utf8 | thread name given by the program |
| kind | Utf8 | `root` \| `spawn` |
| started_ns, ended_ns? | UInt64 | |
| started_at?, ended_at? | Timestamp(ns, UTC) | |
| end_status? | Utf8 | `completed` \| `cancelled` \| `errored`; NULL if no end recorded |
| **execution-level (root rows only)** | | |
| stream_id | Utf8 | the process that ran it |
| engine_id | UInt64 | engine instance inside that process |
| program_id?, revision_id?, source_label? | Utf8 | which compiled program |
| runtime_id? | Utf8 | `baml_id_1_…` the program saw at the root |
| entry_function_id?, entry_fqn? | UInt32 / Utf8 | the function the host called |
| status | Utf8 | `running` \| `abandoned` \| `succeeded` \| `failed` \| `cancelled` \| `panicked` |
| index_state | Utf8 | `complete` \| `no_root_ended` \| `root_started_lost` \| `index_corrupt` (is the record of this execution whole?) |
| duration_ns? | UInt64 | |
| total_calls, total_errors, total_cancelled | UInt64 | sums over the whole CCT |
| calls_retained, threads_total | UInt64 | |
| value_state | Utf8 | `complete` \| `partial` \| `none` (were captured values lost?) |
| data_first_seq, data_last_seq, data_file_count | UInt64 | storage bookkeeping |

### `call_path_stats` (alias `cct_population`) — the calling-context tree

One row per distinct *path*: (parent call path, function, call site, call/spawn
edge). Repeated calls update counters; they never add rows.

| column | type | meaning |
|---|---|---|
| execution_id, call_path_id | Utf8 | key |
| parent_call_path_id? | Utf8 | NULL at the root |
| depth | UInt32 | 0 = root |
| function_id, fqn?, definition_key?, kind?, origin? | UInt32 / Utf8 | which function (`kind`: `bytecode` \| `sysop` \| `native`; `origin`: `user_defined` \| `companion` \| `internal` \| `builtin` \| `auto_derive`) |
| call_site_file?, call_site_line?, call_site_start?, call_site_end? | Utf8 / UInt32 | where it was called from |
| edge_kind | Utf8 | `root` \| `call` \| `spawn` |
| calls_started | UInt64 | how many times |
| calls_selected | UInt64 | how many of those were selected for exact records (≥ the resulting `calls` rows if any were lost — see `health`) |
| completed_ok, completed_error, completed_cancelled, completed_exit | UInt64 | outcomes |
| inclusive_ns | UInt64 | total time in this call path incl. children |
| direct_child_ns | UInt64 | time in synchronous children |
| await_ns, await_count | UInt64 | time suspended |
| self_ns | UInt64 | inclusive − children − await (derived) |
| timing_complete | Boolean | false if a counter saturated |
| overflow_reason? | Utf8 | only on synthetic rows for calls the profiler could not attribute |

### `calls` (alias `retained_calls`) — exact records of selected calls

| column | type | meaning |
|---|---|---|
| execution_id, call_id | Utf8 | key |
| parent_call_id?, thread_id | Utf8 | |
| call_path_id? | Utf8 | the `call_path_stats` row this call belongs to |
| function_id, fqn?, definition_key?, kind? | | |
| edge_kind, call_site_* | | as in `call_path_stats` |
| started_ns, ended_ns?, duration_ns? | UInt64 | |
| started_at?, ended_at? | Timestamp | |
| status? | Utf8 | `ok` \| `errored` \| `cancelled` \| `exited`; NULL if no end recorded |
| selection_reasons | List<Utf8> | why it was kept: `root` \| `llm` \| `manual` |
| roles | List<Utf8> | which values policy wanted: `input` \| `output` \| `error` |
| runtime_ids | List<Utf8> | `baml_id_1_…` values installed on this call |
| args_state, output_state, error_state | Utf8 | `available` \| `not_captured` \| `lost:<reason>` \| `not_applicable` |
| args_cid?, output_cid?, error_cid? | Utf8 | content id of each value (joinable, no loading) |
| **args, output, error** | value (virtual) | the captured values — subscript with `['field']` / `[N]`, compare with `=`, `<`… |
| error_id? | Utf8 | → `errors` |
| error_lost_reason? | Utf8 | if the error record could not be kept |

`args` is a named-argument object: `args['customer']['age']`. A missing path
or a captured `null` is simply a non-match; a value that could not be loaded
is reported in the query outcome, never silently treated as NULL.

### `errors` — one row per captured error

| column | type | meaning |
|---|---|---|
| execution_id, error_id | Utf8 | key |
| throw_call_id, throw_thread_id, throw_call_path_id?, throw_function_id, throw_fqn? | | where it was thrown |
| throw_site_file?, throw_site_line?, … | | |
| kind | Utf8 | `fresh` \| `rethrow` |
| source | Utf8 | `bytecode` \| `native_call` \| `engine_call` \| `future_resume` |
| value_state, value_cid?, **value** (virtual) | | the error value |
| stack_complete | Boolean | |
| stack | List<Utf8> | fqns from the root down to the throw |
| terminal_call_ids | List<Utf8> | retained calls that ended because of this error |

### `function_definitions` — the compiled program's function table

| column | type | meaning |
|---|---|---|
| program_id, function_id | Utf8 / UInt32 | key |
| fqn, display_name | Utf8 | |
| definition_key? | Utf8 | stable identity across recompiles |
| kind, kind_detail?, origin | Utf8 | |
| source_file?, source_start?, source_end? | Utf8 / UInt32 | |
| package?, namespace | Utf8 / List<Utf8> | |
| revision_id?, source_label? | Utf8 | |

### `health` — did the profiler lose anything? (long format)

| column | type | meaning |
|---|---|---|
| execution_id, metric | Utf8 | key |
| plane | Utf8 | `execution` \| `cct` \| `overflow` \| `process` \| `data` |
| value | UInt64 | |
| edge_kind?, reason? | Utf8 | for overflow rows |

Rows: the profiler's loss/saturation counters (e.g. `evidence_queue_full`,
`value_attempt_transport_exceeded`, `corrupt_records`), CCT saturation flags,
one row per overflow bucket, and the data-completeness result of reading the
execution. Zero rows with `value > 0` ⇒ nothing was lost.

### Views (SQL on top of the tables)

| view | definition |
|---|---|
| `hot_call_paths` | `SELECT execution_id, fqn, call_path_id, self_ns, inclusive_ns, calls_started FROM call_path_stats WHERE overflow_reason IS NULL AND timing_complete ORDER BY self_ns DESC` |

(No `executions` view ships today — decided 2026-08-24; use
`threads WHERE parent_thread_id IS NULL`.)

### Internal tables (`BAML_INTERNAL` / playground only)

`processes` (one per process: `stream_id, pid, zero_unix_ns, baml_version,
os_arch, alive, meta_hw, data_hw`), `store_files` (one per file on disk),
`value_index` (one per stored value) — for debugging the store itself.

---

## 3. How the tables relate

~~~text
threads (root row = execution) ──execution_id──┬── contexts ──call_path_id──┐
                                               ├── calls ─────────────────┘  calls.parent_call_id → calls
                                               ├── errors   errors.throw_call_id → calls, throw_call_path_id → contexts
                                               └── health
threads.spawn_call_id → calls                   (program_id, function_id) → functions
~~~

- Everything execution-scoped carries `execution_id`, so no parent-walking is
  needed to scope a query to one execution.
- Across executions and processes, group by `definition_key` or `fqn`
  (`function_id`/`call_path_id` are per-*build*: `program_id` is a conservative
  content hash of the compiled sources — any byte change, comments included,
  is a new program — so `call_path_id` is comparable across runs of one build
  and deliberately not across edits).

---

## 4. The data structures underneath (for the curious)

| SQL concept | on disk (`.baml/profiles-v1/`) |
|---|---|
| `threads` root rows, `status`, `health` | `streams/<process>/meta/*.bamlmeta`: `RootStarted` / `RootEnded` records (+ `StreamStarted` for wall-clock zero, `EngineStarted` for program id and function table) — the index; listing never opens data files |
| `threads` non-root rows | `ThreadStart` / `ThreadEnd` facts in `data/*.bamldata` |
| `call_path_stats` | CCT deltas (context key, parent, function, call site, edge, counters) merged by key across all data segments of the execution |
| `calls` | `SpanStart` / `SpanEnd` / `SpanRuntimeId` / `ValueOccurrence` / `TerminalErrorRef` facts |
| `errors` | `ErrorCapture` facts; `stack` is rebuilt by walking the CCT parent chain |
| `function_definitions` | one `FunctionTableV1` object in `cas/sha256/…` per compiled program (deduplicated) |
| `args` / `output` / `error` | value bodies in `cas/sha256/<cid>.bamlvalue`; the SQL column holds an opaque handle (`0x01 ‖ codec ‖ cid`, or `0x00 ‖ reason` when unavailable) that the engine resolves in batches only when a query touches it |

Value columns are Arrow `Binary` with field metadata `baml.virtual = value`;
the planner rewrites `args['a'][0] >= 30` into internal functions that load
and compare the value, and renders a bare `SELECT args` as JSON. Every other
column is an ordinary Arrow column (Utf8, UInt64, Boolean, List, Timestamp).

Status/completeness semantics: `status` comes from `RootEnded` if present,
otherwise `running` while the writing process holds its `stream.lock` and
`abandoned` once it doesn't; `index_state` says whether the index records are
whole; the `health` rows (`plane = data`) say whether the execution's data
data files were all present and readable.
