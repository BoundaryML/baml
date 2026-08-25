# `baml query` — Scope and Implementation Plan

Status: scoping / handoff plan for the SQL query layer over the segmented
profiling backend. Stacks on `paulo/re-profiling-backend`
(`TASK/profiling-backend-mvp.md`). Nothing in this document is implemented on
this branch yet; "Ground truth" below is verified against code at `7a351097c`.

The material under `TASK/reference/` (Project Studio canon, 2026-08-10) and
the two prior implementations (PR #4343, `origin/paulo/cct-1`) were used as
inputs. Where this document disagrees with them, this document wins for the
stack; disagreements are listed explicitly in Sections 2.3 and 11.

---

## 0. Decisions at a glance

| # | Decision | Why |
|---|---|---|
| S1 | **DataFusion is the engine.** SQL parsing, planning, joins, aggregates, LIMIT, streaming `RecordBatch` output all belong to DataFusion; we write table providers, a value planner, and a thin session shell. | Reverses the 2026-07 "hand-rolled executor" design. Confirmed direction from Paulo; two prototypes (PR #4343, cct-1) already proved the integration shape on DataFusion 54. |
| S2 | **Two crates + CLI verb.** `baml_query` (backend-neutral core: catalog model, session, value lowering, budgets, outcome) and `baml_query_profiles` (local provider over `.baml/profiles-v1`). `baml_cli` gains `baml query`. | Decoupling rule: the core must compile without `bex_events`, `bex_engine`, `baml_cli`, SQLite, AWS, ClickHouse. A hosted provider later is a third crate implementing the same traits. |
| S3 | **Virtual tables are declared, not inferred.** A `Catalog` is a typed registry of `RelationDef`s (name, alias, grain, columns, docs, visibility) plus SQL-defined **views**. Hosts pick a `CatalogProfile` (`public` / `internal`) to choose what users see. | "Easily configure the virtual tables users can see" = edit one Rust table or add a view in SQL; no provider code for derived tables. |
| S4 | **Lazy, per-execution, pushdown-aware providers.** No eager materialization of every execution at session build (cct-1 smell). Providers prune by `execution_id` / status / time from the meta plane and fold one execution at a time, streaming batches. | `.baml/profiles-v1` grows without bound; `SELECT 1` must not read every segment. |
| S5 | **Values are opaque handles + a neutral value model.** Value columns are Arrow `Binary` handles with field metadata; subscripts/comparisons are lowered by an `ExprPlanner` into internal `__baml_*` UDFs; hydration is batched per `RecordBatch` through a `ValueResolver`. The core owns a small `Value` enum; the provider decodes CAS bodies into it. | Ports the cct-1 D7 design that already passes its conformance gates; removes the `bex_events::CanonValue` dependency from the core. |
| S6 | **Catalog v1 = what the backend persists after P0** (`TASK/profiling-backend-streams.md`): `threads` (a parentless thread *is* an execution — there is no separate "run"), `call_path_stats`, `calls`, `errors`, `function_definitions`, `health` (+ the `hot_call_paths` view; `llm_calls` amended out 2026-08-24 — no producer emits an LLM function kind yet, see §4.2; **no `executions` view** — decided 2026-08-24, the idiom is `threads WHERE parent_thread_id IS NULL`). Versioned names (`threads_v1`) with unversioned aliases. | Honest columns only; NULL with a typed reason, never fabricated; one identity scheme (`ThreadRef`/`CallRef`/`ContextKey`). |
| S7 | **Typed terminal outcome on every stream; exit codes 0–5.** Unavailable values are counted, never silently NULL. | Agent-facing contract from the canon (D12/D13/IN-Q2-3), already implemented in cct-1. |
| S8 | **Native only in v1.** `baml query` (CLI) and the playground server embed the engine; wasm/browser never links DataFusion. | Dependency weight; the reader is `cfg(not(wasm32))` already. |

---

## 1. Goal and non-goals

**Goal.** An agent (or a human) can run portable SQL against every execution
the local profiler has recorded for a project, discover the schema without
guessing, join population aggregates (CCT) with exact retained calls, filter
on captured values, and always know whether the answer is complete.

~~~text
baml query --schema --format json
baml query "SELECT thread_id, status, total_errors FROM threads WHERE parent_thread_id IS NULL ORDER BY started_at DESC LIMIT 20"
baml query "SELECT fqn, sum(self_ns) AS self_ns FROM call_path_stats GROUP BY fqn ORDER BY self_ns DESC LIMIT 10"
baml query "SELECT call_id, args['customer']['age'] AS age FROM calls WHERE fqn = 'ScoreCustomer' AND args['customer']['age'] >= 30" --format jsonl
~~~

**Non-goals for this stack** (tracked, not forgotten):

- hosted/ClickHouse provider and `--hosted` routing (traits shaped for it;
  no implementation);
- writing data from SQL, user-defined functions, `CREATE VIEW` by users;
- a per-invocation row for every call (the backend is population-first by
  design; `calls` is the *retained* subset);
- reading the legacy `.baml/history` plane (logs, `RunStarted`) — logs enter
  SQL only once they are evidence facts in `profiles-v1`;
- in-browser (wasm) execution; the playground reaches the engine through the
  playground server;
- retention/GC/`baml clean runs` semantics (the backend MVP defers them).

---

## 2. Ground truth

### 2.1 What the backend persists

At `7a351097c` the store is the MVP's per-boundary layout
(`runs/<boundary_id>/{run.meta, cct/*.bamlcct, evidence/*.bamlspans, run.end}`
+ `cas/`), read by `DurableRunReader::load() -> ProfileRun`. That layout is
**superseded by P0** (`TASK/profiling-backend-streams.md`), which this plan
targets:

~~~text
.baml/profiles-v1/
  streams/<process_euid hex>/
    stream.lock                      held while the owning process is alive
    meta/<seq>.bamlmeta              StreamStarted (wall-clock zero), EngineStarted (program_id,
                                     function_table_cid), RootStarted/RootEnded per execution
                                     (status, health, data segment range)
    data/<seq>.bamldata              groups keyed by root ThreadRef: CCT deltas + evidence facts
                                     (SpanStart/End, RuntimeId, ValueOccurrence, ErrorCapture,
                                     TerminalErrorRef, ThreadStart, ThreadEnd)
  cas/sha256/<2 hex>/<64 hex>.bamlvalue   codec 1 = value body, codec 2 = FunctionTableV1
~~~

Reader API after P0 (streams spec §6): `list_executions(root) ->
Vec<ExecutionSummary>` (meta planes only), `StreamReader`,
`ExecutionReader::load() -> ExecutionProfile { contexts, overflow, threads,
spans, errors, summary }`, `read_value(cid)`, `function_table()`. An
execution is identified by its root `ThreadRef` (`baml_thread_1_…`); the
host's `baml_id_1_…` runtime token is stored on the root (`RootStarted.
runtime_id`) for lookups from the playground/logs.

Queryable per execution: identity, program/engine, wall-clock start/end,
status + completeness + 26 health counters; CCT rows (context key, parent,
`function_id` → fqn via the function table, call site, edge, counters,
derived self time); overflow buckets; exact spans for root / LLM /
`$id`-selected calls with values as CID; thread lineage (parent, spawn call,
spawn site, name, end status); error captures with CCT-reconstructed stacks;
value bodies by CID.

### 2.2 Gaps a SQL layer hits immediately (verified)

| Gap | Evidence | Consequence |
|---|---|---|
| G0 **Identity and per-execution file cost.** The MVP keys storage by a second random id (`BoundaryId`), names the artifact a "run", and pays 4 files / 8 fsyncs per execution, two of them synchronously on the root call path. | streams spec §0 | **P0**: execution = parentless thread, one stream per process. |
| G1 **No function names on disk.** `FunctionId(u32)` indexes an in-memory `FunctionMetadataTable` (`metadata.rs:93`) that is never written to `profiles-v1`; no file-id→path table either. | grep: no writer of the table in store/session | `fqn`, `definition_key`, `kind`, source path are unavailable → every user-facing query is by opaque integer. **P0** (`EngineStarted.function_table_cid`, CAS codec 2). |
| G2 **No wall clock.** `started_ns`/`ended_ns` are process-relative; `clock::started_at_epoch_ns()` is not persisted. | `decoder.rs:1072,1243` | `started_at`, "last hour" filters impossible. **P0** (`StreamStarted.zero_unix_ns`). |
| G3 `ProgramId` is random per engine instance; `revision_label`/`source_label` are always `None`. | `bex_engine/src/lib.rs:1692-1694` | `ContextKey` is not comparable across engines; cross-execution grouping must go through `definition_key`/`fqn` (P0 function table). `ProgramId` is a conservative content hash after P0 (streams spec §2.3), so `call_path_id` is comparable across executions of one build. |
| G4 Store root defaults to CWD-relative `.baml/profiles-v1` in the engine; `baml clean` uses the project root. | `sizing.rs:20-32`, `clean_command.rs:20` | `baml run` from a subdirectory writes where `baml query` won't look. **P0 §7.5** (CLI resolves the project root; `flush_and_join` at exit). |
| G5 No per-call rows for ordinary calls; values only for selected calls. | design (MVP §1.1) | Catalog must be honest: `calls` is "retained", `call_path_stats` is the population. (Thread lineage *is* durable after P0: `ThreadStart`/`ThreadEnd`.) |
| G6 No liveness signal for an unfinished execution read from another process. | `reader.rs`, MVP §10 | **P0 §6.4**: `stream.lock` held ⇒ `running`, released ⇒ `abandoned`; completeness from `RootEnded`. |
| G7 Value bodies are prost `BamlOutboundValue` (codec 1); the encoder is private to `bex_engine`, proto lives in `bridge_ctypes`. `bex_events::value` already references the type. | `trace_value_encode.rs`, `bex_events/src/value/encode.rs` | A decoder into a neutral value model must live where the provider can reach it without `bex_engine` (**B4**). |
| G8 No SQL/columnar dependency anywhere in either workspace lockfile. | `Cargo.lock` grep | DataFusion is net-new weight (Section 9). |

### 2.3 Prior art and what we take from it

**PR #4343** (`codex/local-query-engine`, draft, conflicting, no reviews) —
DataFusion 54.1 over a read-only **SQLite** physical store with SHA-256 blob
hydration. *Take:* the `TableProvider` + `LazyMemoryExec`/`LazyBatchGenerator`
+ `supports_filters_pushdown` pattern; the `QueryBudgets`/`QueryMetrics`
shape; the idea of caller-defined logical→physical column mappings; the
`$value_ref` recursive hydrator with cycle/depth/bytes checks. *Drop:* SQLite
and everything in `resident.rs`/`pushdown.rs` (SQLite SQL text), the invented
13-column `function_calls` schema, OFFSET pagination, `block_on` hydration
inside the generator, the per-batch (not per-query) budgets, the shared
cancellation token that bricks the engine, the `contains` UDF that shadows
DataFusion's string `contains`.

**`origin/paulo/cct-1`** (`baml_query`, `baml_query_local`,
`baml_cli/src/query_command.rs`; commits `e018555a2`, `7bf85e80f`,
`32b9fe5fd`) — the "Q1/Q2 built" claims in `TASK/reference` refer to this
branch. It is built over the *old* `bex_query`/`.bamlmeta`/BLAKE3-canon
store, all of which is gone on this stack. *Take, near-verbatim:* `catalog.rs`
(RelationDef/ColumnDef model, value-column metadata convention, versioned
name + alias), `session.rs` (gatekeeper, `ExprPlanner` registration,
capability/authorization walks, `TrustedRelation`, streaming `QueryExecution`
+ outcome), `provider.rs` (`RelationProviderFactory`, `PushdownClass`),
`budget.rs`, `capability.rs`, `outcome.rs`, `error.rs`, `value/lowering.rs`,
`value/semantics.rs`, the `tests/q1_gates.rs` conformance corpus, the CLI
command shape and exit codes. *Rewrite:* all of `baml_query_local` (bound to
old readers), the `CanonValue`/BLAKE3 CID assumptions in the resolver and the
`__baml_vcmp_cid` shortcut, eager per-session `MemTable` materialization,
per-row synchronous hydration, string-keyed `match relation.name` coupling.

**`TASK/reference`** (Project Studio canon) — keep D3/D5/D6/D7/D10/D12/D13/
D14/D16 (engine, backend-neutral crate, value operators, fixed snapshot,
typed unknowns, mandatory outcome, platform-only functions, versioned names)
and the IN-Q2-3 CLI contract. Not carried: `cct_windows`, `observations_*`,
`exact_windows`, `spawn_instances`, `call_sites_v1` as tables (the backend
has no producers for them); `evidence_issues_v1` is replaced by `health`
(long format over the 26 counters + overflow buckets) until a grouped issue
ledger exists.

---

## 3. Architecture

### 3.1 Crates and dependency rules

~~~text
baml_language/crates/
  baml_query/            core, backend-neutral
    src/catalog/   {mod,relation,config,v1,views}.rs   RelationDef, ColumnDef, Catalog, CatalogProfile, ViewDef
    src/session/   {builder,gatekeeper,exec,schema_provider}.rs
    src/provider.rs      RelationProviderFactory, PushdownClass, TrustedRelation
    src/value/     {model,handle,resolver,lowering,semantics,render}.rs
    src/budget.rs  src/outcome.rs  src/error.rs  src/scope.rs  src/capability.rs
    tests/conformance/   fixture provider + resolver; the q1 gates, extended
  baml_query_profiles/   local provider over .baml/profiles-v1
    src/universe.rs      bind project → {streams, executions (meta planes only), generation}
    src/threads.rs src/contexts.rs src/calls.rs src/errors.rs src/functions.rs src/health.rs
    src/fold_cache.rs    per-execution lazy fold, LRU keyed by (execution_id, data_first..=data_last, ended?)
    src/resolver.rs      CAS → Value, batched
    tests/e2e/           baml run → baml query goldens
  baml_cli/src/query_command.rs
~~~

Dependency rules (enforced by a test that reads `Cargo.toml`, as cct-1 did):

- `baml_query` depends on `datafusion` (pinned `=54.1.0` — cct-1's 55.0.0
  declares `rust-version = 1.94` and the workspace toolchain is pinned to
  1.93.0; 54.1.0 is the newest that builds — default-features = false,
  features `sql`, `nested_expressions`, `unicode_expressions`,
  `regex_expressions`, `datetime_expressions`), `arrow`, `serde`,
  `serde_json`, `tokio` (`rt`, `sync`), `futures`, `thiserror`. It must **not** name `bex_events`,
  `bex_engine`, `baml_cli`, `rusqlite`, `clickhouse`, `aws`.
- `baml_query_profiles` depends on `baml_query` + the store/reader leaf crate
  (`bex_prof_store` after B0; `bex_events` until then) + the value decoder
  location chosen in B4. Never on `bex_engine`/`bex_vm`.
- `baml_cli` depends on both; `baml_lsp_server` (playground server) may
  depend on both for an `/api/obs/query` endpoint (Phase 4).
- Nothing under `bridge_wasm`/`sys_wasm` may depend on either crate; CI gate:
  `cargo check -p bridge_wasm --target wasm32-unknown-unknown` stays green
  and `cargo tree -p bridge_wasm -i datafusion` is empty.

DataFusion version: pin the newest 5x line that compiles on the workspace
toolchain (1.93) at implementation time (54.1 is proven by both prototypes;
the local index currently lists 55.0). Pin with `=` in the workspace manifest
— DataFusion minor bumps change planner behaviour.

### 3.2 Request flow

~~~text
CLI / playground server
  │  resolve project root → .baml/profiles-v1
  ▼
baml_query_profiles::bind(root, BindOptions) ──► Universe { streams: Vec<BoundStream{id, header, alive, meta_hw, data_hw}>,
  │                                               executions: Vec<ExecutionSummary>, generation }   (meta planes only; no data segment read)
  ▼
baml_query::QuerySessionBuilder::new(catalog_profile, scope, snapshot, resolver, factory)
  .with_budgets(..).with_cancellation(..).build()
  │  SessionContext with: BamlSchemaProvider (lazy), ExprPlanner (values), UDFs, information_schema
  ▼
session.execute(sql) ──► gatekeep → plan → rewrite bare value cols → capability/authz checks → stream
  │                                              ▲
  │   TableProvider::scan(projection, filters, limit) ──► prune executions by execution_id/status/time
  │                                              │         fold one execution at a time (fold_cache), emit RecordBatches
  │   __baml_path / __baml_vcmp* UDFs ──► HydrationContext::resolve_many(handles) ──► resolver (CAS) ──► Value
  ▼
QueryExecution { next_batch(), finish() -> QueryOutcome }   exit code from outcome
~~~

---

## 4. Catalog v1

### 4.1 Declaration model (the "configure virtual tables" lever)

~~~rust
pub struct ColumnDef { name, data_type: DataType, nullable, key: bool, value_role: Option<ValueRole>, doc, visibility: Visibility }
pub struct RelationDef { name: "threads_v1", alias: "threads", grain: Grain, columns: Vec<ColumnDef>, doc, visibility: Visibility, provisional: bool }
pub struct ViewDef     { name, alias, sql: &'static str, doc, visibility }   // SQL over other relations/views
pub enum Visibility    { Public, Internal, Hidden }
pub struct Catalog     { version: "v1", relations: Vec<RelationDef>, views: Vec<ViewDef> }
pub struct CatalogProfile { base: Catalog, show: Visibility /* max visibility level */, overrides: Vec<Override> }
pub enum Override      { HideRelation(&str), HideColumn(&str, &str), ExposeInternal(&str), AddView(ViewDef) }
~~~

- `catalog::v1()` is the frozen default. `CatalogProfile::public()` shows
  `Public`; `CatalogProfile::internal()` (CLI under `BAML_INTERNAL`, the
  playground) also shows `Internal` relations such as `store_files_v1` and raw
  health counters.
- Views are registered as DataFusion `ViewTable`s from their SQL at session
  build; they are how we add convenience tables (`hot_call_paths`, and
  `llm_calls` once an LLM function kind exists) without provider code. Views are validated against the
  catalog by a golden test (every view plans).
- `--schema` renders the *profile* (relations + views + columns + docs), so
  what an agent discovers is exactly what it may query.
- Column-level hiding exists so hosts can remove provider-private columns
  (e.g. future tenant scope) without forking the catalog.

Value columns carry Arrow field metadata `baml.virtual = value`,
`baml.role = input|output|error`; that metadata is the only thing the planner
keys on (cct-1 convention, kept).

### 4.2 Relations

Names are `<name>_v1` with alias `<name>`. `id` columns are Utf8 wire strings
(`baml_thread_1_…`, `baml_call_1_…`; the host runtime token is `baml_id_1_…`).
`call_path_id` is Utf8 hex of `ContextKey` (no wire form exists yet — see Q2).
`*_ns` are `UInt64` (saturating from `u128` with `timing_complete = false`
on saturation) and are process-relative; `*_at` are `Timestamp(ns, UTC)` =
`StreamStarted.zero_unix_ns + *_ns` (NULL only if the stream header is
missing). **`execution_id` is the root thread's `thread_id`** — every
execution-scoped relation carries it so rows join without walking parents.

**`threads_v1`** — one row per logical thread (`ThreadStart`/`ThreadEnd`
facts). **A thread with `parent_thread_id IS NULL` is an execution**; the
execution-level columns below are non-NULL only on those rows. Key
`(execution_id, thread_id)`.

| column | type | source |
|---|---|---|
| execution_id, thread_id | Utf8 | root `ThreadRef` / this thread's `ThreadRef` |
| parent_thread_id?, spawn_call_id?, spawn_function_id?, spawn_fqn? | Utf8/UInt32 | `ThreadStart.parent`, `.spawn_call` (+ function table) |
| spawn_site_file?, spawn_site_line? | Utf8/UInt32 | `ThreadStart.spawn_site` |
| name? | Utf8 | `ThreadStart.name` (empty → NULL) |
| kind | Utf8 | `root|spawn` |
| started_ns, ended_ns? | UInt64 | `ThreadStart`/`ThreadEnd` |
| started_at?, ended_at? | Timestamp | via stream header |
| end_status? | Utf8 | `completed|cancelled|errored` (`ThreadEnd`), NULL if no end fact |
| *execution-level (root rows only):* | | |
| process_id | Utf8 | `process_euid` hex |
| engine_id | UInt64 | `ThreadRef.engine_id` |
| program_id?, revision_id?, source_label? | Utf8 | `EngineStarted` |
| runtime_id? | Utf8 | `RootStarted.runtime_id` (`baml_id_1_…`, what `baml.id.current()` returned at the root) |
| entry_function_id?, entry_fqn? | UInt32/Utf8 | the root span's `SpanStart.function_id` (+ function table); NULL if the root span is not retained |
| status | Utf8 | `running|abandoned|succeeded|failed|cancelled|panicked` (streams spec §6.2) |
| index_state | Utf8 | `complete|no_root_ended|root_started_lost|index_corrupt` (meta plane only) |
| duration_ns? | UInt64 | root span inclusive, else `ended_ns - started_ns` |
| total_calls, total_errors, total_cancelled | UInt64 | Σ CCT counters (incl. overflow) |
| calls_retained, threads_total | UInt64 | spans with `SpanStart`; `ThreadStart` count |
| value_state | Utf8 | `complete` / `partial` (any `ValueState::Lost`) / `none` |
| data_first_seq, data_last_seq, data_file_count | UInt64 | `RootEnded` |

**`call_path_stats_v1`** (the CCT population; alias `call_path_stats`, secondary alias
`cct_population` kept for canon readers). Key `(execution_id, call_path_id)`.

| column | type | source |
|---|---|---|
| execution_id, call_path_id | Utf8 | root `ThreadRef`; `ContextKey` hex |
| parent_call_path_id? | Utf8 | `ContextTuple.parent_context_key` |
| depth | UInt32 | derived by walking parents (0 = root) |
| function_id | UInt32 | `ContextTuple.function_id` |
| fqn?, definition_key?, kind?, origin? | Utf8 | function table (`EngineStarted.function_table_cid`) |
| call_site_file?, call_site_line?, call_site_start?, call_site_end? | Utf8/UInt32 | `CallSiteSourceSpan` + file table |
| edge_kind | Utf8 | `root|call|spawn` |
| calls_started, calls_selected, completed_ok, completed_error, completed_cancelled, completed_exit | UInt64 | `CctCounters` (`calls_selected` = the `spans_selected` counter: activations capture policy selected; ≥ actual `calls` rows when records were lost) |
| inclusive_ns, direct_child_ns, await_ns, self_ns | UInt64 | counters; `self_ns` derived |
| await_count | UInt64 | |
| timing_complete | Boolean | `DerivedTiming.complete` && !saturated |
| overflow_reason? | Utf8 | non-NULL only for the synthetic overflow rows (`call_path_id = 'overflow:<reason>:<edge>'`) |

**`calls_v1`** (retained exact spans; alias `calls`, secondary alias
`retained_calls`). Key `(execution_id, call_id)`.

| column | type | source |
|---|---|---|
| execution_id, call_id, parent_call_id?, thread_id | Utf8 | `SpanStart` |
| call_path_id? | Utf8 | `ContextRef::Normal(key)`; NULL for overflow contexts (+ `call_path_overflow_reason?`) |
| function_id, fqn?, definition_key?, kind? | | function table |
| edge_kind | Utf8 | |
| call_site_* | | as contexts |
| started_ns, ended_ns?, duration_ns? | UInt64 | process-relative |
| started_at?, ended_at? | Timestamp | via stream header |
| status? | Utf8 | `SpanEnd.status` (`ok|errored|cancelled|exited`); NULL = no end fact |
| selection_reasons | List<Utf8> | `root|llm|manual` |
| roles | List<Utf8> | `input|output|error` |
| runtime_ids | List<Utf8> | initial + `SpanRuntimeId` overrides (`baml_id_1_…`) |
| args_state, output_state, error_state | Utf8 | `available|not_captured|lost:<reason>|not_applicable` |
| args_cid?, output_cid?, error_cid? | Utf8 | `ValueCid` wire — resident identity, joinable without hydration |
| args, output, error | Binary (virtual) | handle `0x01 ‖ codec ‖ cid` / `0x00 ‖ reason` |
| error_id? | Utf8 | `TerminalErrorRef::Capture` |
| error_lost_reason? | Utf8 | `TerminalErrorRef::Lost` |

`args` is a named-argument object (`args['customer']`); `args[0]` is a
planning error with a remedy (cct-1 rule, kept).

**`errors_v1`** — one row per `ErrorCapture`. Key `(execution_id, error_id)`.
Columns: `execution_id, error_id, throw_call_id, throw_thread_id,
throw_call_path_id?, throw_function_id, throw_fqn?, throw_site_*`, `kind
(fresh|rethrow)`, `source (bytecode|native_call|engine_call|future_resume)`,
`value_state`, `value_cid?`, `value` (virtual, role `error`), `stack_complete`
Boolean, `stack` List<Utf8> (fqns root→throw via `error_stack`),
`terminal_call_ids` List<Utf8> (spans whose `TerminalErrorRef` targets this
capture).

**`function_definitions_v1`** — one row per `(program_id, function_id)` from the
function table (`FunctionTableV1`, streams spec §4.6). Columns: `program_id,
function_id, fqn, display_name, definition_key?, kind, kind_detail?, origin,
source_file?, source_start?, source_end?, package?, namespace, revision_id?,
source_label?`. Joined from `call_path_stats`/`calls`/`errors`/`threads` through the
execution's `program_id`.

**`health_v1`** — long format. Key `(execution_id, metric)`. Columns:
`execution_id, plane (execution|cct|overflow|process|data), metric (Utf8 counter
name), value UInt64, edge_kind?, reason?`. Rows: the 26
`ExecutionHealthSnapshot` counters from `RootEnded`, the three `CounterHealth`
flags (as 0/1), one row per overflow bucket (`calls_started`), and the
execution's `data_file_count`, and `plane = data` rows `data_state`
(`complete|incomplete`) and one row per `DataIssue` (these come from `load()`;
`threads` itself never requires a data fold). This replaces the canon's
`evidence_issues_v1` until the backend has a grouped issue ledger.

**Internal** (`CatalogProfile::internal()` only): `processes_v1` (process_id,
os_pid, zero_unix_ns, baml_version, os_arch, alive, meta_hw, data_hw),
`store_files_v1` (process_id, plane, sequence, path, record_or_group_count,
payload_len, checksum_ok, decode_ok), `value_index_v1` (cid, codec, body_len,
path) — debugging the store from SQL.

**Views (v1 ships with):**

~~~sql
CREATE VIEW hot_call_paths AS
  SELECT execution_id, fqn, call_path_id, self_ns, inclusive_ns, calls_started FROM call_path_stats
  WHERE overflow_reason IS NULL AND timing_complete ORDER BY self_ns DESC;
~~~

Amended 2026-08-24: **`llm_calls` does not ship in v1.** The originally
planned `CREATE VIEW llm_calls AS SELECT * FROM calls WHERE kind = 'llm'`
has no backend truth — `FunctionTableV1`'s kind codes are
`bytecode|sysop|native|native_unresolved`; no producer emits an LLM
function kind yet, so the view could only ever return an empty (and
misleading) result. Re-adding it once the kind exists is additive and
stays v1.

Decided 2026-08-24: **no `executions` view** ships today — root threads are
selected with `WHERE parent_thread_id IS NULL`, and `--schema`/docs teach
that idiom. Adding the view later is a one-line, fully reversible change.

### 4.3 Joins

`threads(execution_id, thread_id)` ← every execution-scoped relation via
`execution_id` (= the root thread's `thread_id`), `calls.thread_id`,
`errors.throw_thread_id`, `threads.parent_thread_id`/`spawn_call_id`;
`call_path_stats(execution_id, call_path_id)` ← `calls.call_path_id`,
`errors.throw_call_path_id`; `calls(execution_id, call_id)` ←
`calls.parent_call_id`, `errors.throw_call_id`, `errors.terminal_call_ids`,
`threads.spawn_call_id`; `function_definitions(program_id, function_id)` ←
`call_path_stats`/`calls`/`errors`/`threads` via the execution's `program_id`.
Cross-execution grouping: `definition_key` (or `fqn`), never
`function_id`/`call_path_id` (see G3).

### 4.4 Versioning

`catalog::v1()` is frozen by golden tests (relation list, keys, every column
name/type/nullability, every view plans). Additive changes (new nullable
column, new view) stay v1. Renames/removals/type changes create `v2` with
new `RelationDef`s; aliases are pinned to the session's catalog version,
never "latest". `--schema` prints `catalogVersion`.

---

## 5. DataFusion integration

### 5.1 Session

`QuerySessionBuilder::new(profile: CatalogProfile, scope: QueryScope,
snapshot: Snapshot, resolver: Arc<dyn ValueResolver>, factory: Arc<dyn
RelationProviderFactory>)` → `SessionStateBuilder::new().with_default_features()
.with_expr_planners(vec![BamlValuePlanner]).with_information_schema(true)`.
Register: value UDFs (`__baml_path`, `__baml_vcmp`, `__baml_vcmp_value`,
`__baml_vcmp_cid`, `__baml_vcmp_json`, `baml_value_cid`, `baml_value_json`),
capability stubs, a `BamlSchemaProvider` under catalog `baml` / schema
`public` (default), views. One `SessionContext` per session; sessions are
cheap and bound to one snapshot.

Gatekeeper (cct-1, kept): parse with `DFParser` + `GenericDialect`; exactly
one statement; must be a `Query` (or `SHOW TABLES` / `SHOW COLUMNS` /
`DESCRIBE` / `EXPLAIN <query>` — allowed read-only statements); reject
`__baml_` in user text; reject anything DDL/DML with `invalid_sql` and a
remedy.

### 5.2 Lazy schema provider

`BamlSchemaProvider: SchemaProvider` lists the profile's visible relations
and views; `table(name)` calls `factory.provider(relation, &snapshot)` on
first use and caches the `Arc<dyn TableProvider>` for the session. A relation
the backend does not serve resolves to an empty provider over the catalog
schema (still queryable, `--schema` still lists it). `TrustedRelation` wraps
every provider: schema is always the catalog schema; value-bearing filters
are never pushed down.

### 5.3 Table providers and pushdown

Each provider implements `TableProvider::scan(state, projection, filters,
limit)` → a custom `ExecutionPlan` (`ProfilesScanExec`) with one partition per
pruned execution chunk (bounded parallelism, default `min(4, executions)`), streaming
`RecordBatch`es of ~8k rows. `supports_filters_pushdown` classifies:

- `Exact`: `execution_id = 'x'`, `execution_id IN (...)`, `status = ...`,
  `complete`, `process_id = ...` — evaluated at the universe (meta planes only;
  no data segment read);
- `InexactCandidate`: `started_at`/`ended_at` ranges, `fqn =` /
  `function_id =` (prune executions whose engine's function table lacks the
  fqn, still re-filtered by DataFusion);
- `Unsupported`: everything else, including any value predicate.

`limit` is honoured only when every filter was `Exact` (cct-1 gate
`final_limit_never_reaches_the_provider_below_a_value_predicate` is kept and
extended).

### 5.4 Per-execution fold cache

`fold_cache::get(exec) -> Arc<FoldedExecution>` runs
`ExecutionReader::load()` (reads only data segments
`[data_first_seq ..= data_last_seq]` of the execution's stream, skipping
other executions' groups) once per `(execution_id, data_first, data_last,
ended?)` per session, behind a bounded LRU (memory-governed: default 256 MiB
of folded state, configurable by the host). `threads`, `call_path_stats`, `calls`,
`errors`, `health` for the same execution share the fold. Depth and
`self_ns` are computed at fold time. `threads`-only/`executions` queries
never touch data segments: `ExecutionSummary` comes from the meta plane.

### 5.5 Values

- **Model.** `baml_query::value::Value { Null, Bool, Int(i64), Float(f64),
  BigInt(String), String, Bytes, List, Map(Vec<(String, Value)>),
  Class{ name, fields: Vec<(String, Presence, Option<Value>)> }, Enum{name,
  variant}, Media{...}, Omitted(reason) }`. Maps/classes are compared
  key-sorted; NaN == NaN; ±0 distinct; Int vs Float numerically (cct-1
  `semantics.rs`, ported).
- **Handles.** `0x01 ‖ u16 codec ‖ cid[32]` available; `0x00 ‖ reason`
  unavailable. Providers own the encoding; the core treats it as bytes.
- **Resolver.** `trait ValueResolver { fn resolve_many(&self, handles: &[&[u8]],
  caps: DecodeCaps) -> Vec<Resolved>; fn canonical_cid(&self, handle) ->
  Option<[u8;32]> }`. `HydrationContext` dedupes handles within an Arrow
  array, checks budgets, calls `resolve_many` once per batch, caches misses
  and hits in a bounded LRU. UDFs stay synchronous (local reads are files);
  an `AsyncScalarUDF` variant is the hook for a hosted resolver.
- **Lowering.** `BamlValuePlanner: ExprPlanner` folds `col['k'][N]` into
  `__baml_path(col, path_json, role)`, rewrites comparisons to `__baml_vcmp*`,
  renders bare selected value columns (scalars bare, structures canonical
  JSON). Ported from cct-1 with the relation-qualifier fix.
- **CID equality.** `baml_value_cid('bamlv_1_<hex>')` compares against the
  resident `*_cid` column and is an identity test on the *encoded* body. It
  is **not** semantic equality unless codec 1 is byte-canonical (Q3). v1
  documents it as identity; semantic whole-value equality is `args =
  baml_value_json('{…}')` (decode and compare).
- **Unavailable.** Rows whose predicate cannot be decided are excluded and
  counted in `valueEvaluations.byReason`; selected unavailable values render
  NULL and count; `*_state` columns say why without hydration.

### 5.6 Snapshot, running executions, generation

`bind` reads every stream's meta plane (`StreamReader::open`) and records
per stream: header, `alive` (`stream.lock`, streams spec §6.4), committed
`meta_hw`/`data_hw` (contiguous prefix; torn tail ignored while alive), and
per execution its `ExecutionSummary` (`RootStarted`/`RootEnded`, data range).
`Snapshot { catalog_version, generation = sha256(sorted (stream_id, meta_hw,
data_hw)), projected_through = max committed data sequence across streams }`.
Reads are bounded to the bound high-waters, so re-running the same SQL
against an unchanged store is deterministic (gate).

`status` for an execution without `RootEnded` is `running` when its stream is
alive at bind time and `abandoned` otherwise (streams spec §6.2); counters of
a running execution are so-far values. No heuristics: liveness is the lock.

### 5.7 Budgets, cancellation, outcome, errors

Global per query (never per batch): `max_wall`, `max_result_rows`,
`max_candidate_rows`, `max_hydrations`, `max_decoded_bytes`,
`max_decode_depth`, `max_value_bytes`, `max_fold_bytes`. Exhaustion →
`E_QUERY_BUDGET_EXCEEDED`, outcome `budget_exhausted`. `CancellationToken`
per execution (not per engine). `QueryOutcome { queryCompleted, resultState
complete|incomplete|failed|budget_exhausted|cancelled, snapshot,
valueEvaluations{attempted, available, unavailable, byReason}, rowsStreamed,
error{code, message, retryable, remedy} }` (camelCase). Error codes:
`invalid_sql`, `E_BACKEND_CAPABILITY`, `E_QUERY_BUDGET_EXCEEDED`,
`cancelled`, `authorization_denied`, `dependency_unavailable`,
`artifact_corrupt`, `internal` — every `invalid_sql` carries a remedy
(did-you-mean on relation/column names via Jaro-Winkler against the profile,
house style from `describe`).

### 5.8 Discovery

Three equivalent doors, all rendering the same profile: `baml query --schema
[--table T] --format json|table`; in-SQL `SHOW TABLES`, `SHOW COLUMNS FROM
calls`, `DESCRIBE threads` (DataFusion information_schema; column docs are
exposed through a `baml_columns` internal view so agents can `SELECT` docs);
and a `baml describe` topic (`baml describe query`) per the agent-native docs
rule (clap doc comments + insta snapshot + `describe` YAML topic; nothing
under `fern/`).

---

## 6. CLI contract — `baml query`

~~~text
baml query [SQL] [--schema [--table <NAME>]] [--format table|json|jsonl]
           [--project <PATH>] [--explain] [--max-rows N] [--max-wall <DURATION>]
           [--internal]        # BAML_INTERNAL also flips CatalogProfile::internal()
~~~

- SQL positional or `-` to read stdin (agents pipe multi-line SQL).
- `--schema` without SQL prints the profile; with `--table` only that
  relation/view. JSON shape: `{catalogVersion, generation, relations:[{name,
  alias, grain, provisional, doc, columns:[{name,type,nullable,key,virtual,
  role,doc}]}], views:[{name, alias, sql, doc}]}`.
- Output: rows on stdout, outcome on stderr (`table`), or inline: `json` =
  one envelope `{version, rows, queryOutcome}`; `jsonl` = one row object per
  line, terminal `{"queryOutcome": …}` frame. Streaming in `jsonl`/`table`
  (do not collect everything first — fix from cct-1).
- Exit codes (IN-Q2-3, kept): 0 complete · 1 evidence-incomplete · 2
  invalid SQL / unknown table / authorization · 3 budget · 4 cancelled · 5
  internal or dependency (no store, bind failure, corrupt artifact).
- Respects `OutputArgs` (`--output-preset agent`, `BAML_COLOR`), `--project`
  via `project_load::find_project_root_from` (same resolution as `baml clean`).
- Table renderer: fixed width, 60-char cell truncation with explicit `…`
  elision marker, Binary as `0x…`, `--budget`-style soft truncation of row
  count with a trailing elision line (house style from `describe`).

Examples shipped in `after_long_help` and in the describe topic:

~~~text
baml query "SHOW TABLES"
baml query "SELECT thread_id, status, total_errors, started_at FROM threads WHERE parent_thread_id IS NULL ORDER BY started_at DESC LIMIT 20"
baml query "SELECT fqn, sum(calls_started) calls, sum(self_ns) self_ns FROM call_path_stats WHERE execution_id = 'baml_thread_1_…' GROUP BY fqn ORDER BY self_ns DESC LIMIT 10"
baml query "SELECT c.call_id, c.duration_ns, e.stack FROM calls c JOIN errors e ON e.execution_id = c.execution_id AND e.error_id = c.error_id WHERE c.status = 'errored'"
baml query "SELECT thread_id, name, spawn_fqn, started_ns, end_status FROM threads WHERE execution_id = 'baml_thread_1_…' ORDER BY started_ns"
baml query "SELECT call_id, args['customer']['age'] AS age, output FROM calls WHERE args['customer']['age'] >= 30 LIMIT 50" --format jsonl
~~~

---

## 7. Backend prerequisites (stacked PRs on the profiling branch)

Each must keep the MVP crash/perf gates green and amend
`profiling-backend-mvp.md` per its own amendment list.

| # | Change | Spec | Unblocks |
|---|---|---|---|
| **P0** | **Thread-rooted executions and process streams.** Execution = parentless thread (`ThreadRef`), no `BoundaryId`/"run" in durable formats; one stream per process (`streams/<euid>/{meta,data}`), no per-execution files or fsyncs on the root path; durable `ThreadStart`/`ThreadEnd`; `FunctionTableV1` in CAS referenced from `EngineStarted`; wall-clock zero in `StreamStarted`; `RootEnded` with status/health/data range; `stream.lock` liveness; CLI resolves the store root from the project root and calls `flush_and_join` at exit. | `TASK/profiling-backend-streams.md` (complete: formats, writer, reader, gates, MVP amendments) | every relation in §4.2; `fqn` columns; `started_at`; running/abandoned status; thread lineage; listing without reading data |
| **B0** | Carve the store/reader out of `bex_events` into a leaf crate (`bex_prof_store`): `ids.rs` + `prof/backend/*` + `prof/{record,clock,config}.rs`; deps only `sha2, hex, fs2, rustc-hash, smallvec, base64, uuid, crossbeam-channel` (amended 2026-08-24: `config.rs` is read by `session.rs` and is std-only; `crossbeam-channel` is the `DecoderCommand` producer lane, which P0 keeps for non-admission commands); `bex_events` re-exports. The three transport call sites (consumer wake ×2, `configure_global_transport`) go through `prof::backend::hooks` function pointers installed by `bex_events`'s `register_engine_session` shim. Verified: no backend file imports `metadata`/`collector`/`run`/`value`/`history` or `bex_vm_types`/`sys_types`/`bex_external_types`. Function kind/origin codes are codec-level enums (streams spec §4.6), so the leaf crate never depends on VM types. May land before or together with P0 (P0 rewrites the files it moves). | this doc | `baml_query_profiles` depends on a pure format crate; boundary enforced by Cargo |
| **B4** | Neutral value decoder: `BamlOutboundValue` (CAS codec 1) → `baml_query::value::Value`, living in `baml_query_profiles` using the prost types reachable from `bex_events::value`; if that path drags `bridge_ctypes`, split the proto types into a leaf crate. | this doc §5.5 | hydration |
| ~~B5~~ | **Folded into P0** (decided 2026-08-24): conservative source-content `ProgramId` (streams spec §2.3). | streams spec §2.3 | cross-execution `GROUP BY call_path_id` within one build |

## 8. Phases and gates

**Phase 0 — prerequisites (P0, B0, B4)** on the profiling branch. Gates: MVP
codec goldens updated; `cargo test -p bex_events --lib`, `bex_engine
--test profiling_backend` green; wasm check green; packed e2e (`baml_tests/
profiling_e2e`) unchanged perf within thresholds.

**Phase 1 — `baml_query` core.** Port cct-1 modules (Section 2.3) onto the
neutral `Value` model; add `CatalogProfile`, views, lazy `SchemaProvider`,
information_schema, batched `resolve_many`, per-execution cancellation,
`max_fold_bytes`. Gates: conformance corpus (q1 gates + new: views plan,
profile hiding, SHOW/DESCRIBE, batched hydration counts, limit placement,
outcome exactly-once, dependency allowlist); clippy `-D warnings`; `cargo
tree -i datafusion` shows only `baml_query*`, `baml_cli`, `baml_lsp_server`.

**Phase 2 — `baml_query_profiles`.** `bind` (meta planes), fold cache, six
relations + internal three, pushdown classes, resolver over CAS. Gates: e2e
`baml run` → query goldens for every relation (insta); determinism (same
store ⇒ same generation ⇒ same rows); torn-tail/running-execution read;
corrupt data segment ⇒ `complete = false` + outcome incomplete, never a
panic; perf: 1k executions across 50 streams × 10k contexts × 1k spans
synthetic store — `SELECT count(*) FROM threads WHERE parent_thread_id IS NULL`
< 50 ms (no data segment
opened), `SELECT … FROM call_path_stats WHERE execution_id = ?` < 100 ms warm,
first `RecordBatch` of a full `call_path_stats` scan < 500 ms, RSS bounded by
`max_fold_bytes`.

**Phase 3 — CLI.** `baml query`, `--schema`, formats, exit codes, describe
topic, `exit_code_e2e` additions, help snapshots. Gates: `tools_size_gate`
delta recorded and accepted (Q7); `baml query` cold start < 150 ms on an
empty store.

**Phase 4 — hosts and polish.** Playground server `/api/obs/query` (same
session, `CatalogProfile::internal()`, cancellation on socket close);
Telemetry view pivots ("open in SQL"); views catalog grows from real agent
queries; hosted provider trait review (no implementation).

---

## 9. Risks

- **Dependency weight / build time.** DataFusion + arrow add ~1.2k lock
  entries and minutes of cold build to `baml_cli`; PR #4343's size-gate run
  reported +300–420 KB on packed binaries from lock bumps alone. Mitigation:
  `default-features = false` with the minimal feature set, `=` pin, measure
  with `tools_size_gate` in Phase 3, keep DataFusion out of `baml_pack_host`
  and wasm. If the delta is unacceptable, `baml_cli` feature `query`
  (default on for the distributed binary, off for pack hosts).
- **Planner drift across DataFusion versions.** Pin; conformance corpus runs
  on every bump.
- **P0 is a backend redesign** (layout, writer, reader) with a 1 s
  durability window by design; it is specified and gated in
  `TASK/profiling-backend-streams.md` but is the largest item on the stack.
- **CID equality semantics** depend on codec canonicality (Q3).
- **`ContextKey` hex ids are 64 chars** — fine for agents, ugly for humans;
  a `baml_ctx_1_` wire form would align with the other ids (Q2).
- **Fold cost for `call_path_stats` on huge executions** — `load()` merges every
  group in range; bounded by `max_fold_bytes` and the LRU, but an execution with millions of
  contexts will be slow; streaming folds by segment are the fallback.

---

## 10. Open questions for Paulo

Resolved 2026-08-24: ~~`executions` view~~ (not shipped; idiom documented),
~~program identity~~ (conservative content hash, streams spec §2.3),
~~`publish_interval`~~ (1 s), ~~meta-loss policy~~ (tolerate). Remaining:

1. **Names.** Keep the canon's `_v1` + alias scheme and the secondary aliases
   `cct_population`/`retained_calls`?
2. **Context ids.** Hex `ContextKey` (64 chars) vs a new `baml_ctx_1_<b64>`
   wire form vs a per-execution dense `node_id`. I lean `baml_ctx_1_`, added
   to `ids.rs`.
3. **CID literals.** Is codec-1 (`BamlOutboundValue`) byte-canonical for
   equal values? If not, `baml_value_cid(...)` stays an identity test on the
   `*_cid` columns and semantic equality always decodes. Reuse the `bamlv_1_`
   wire prefix for sha256 cids, or a new prefix?
4. **Logs.** Expose logs in v1 (requires the legacy `.baml/history` plane) or
   wait for logs to become evidence facts? This plan says wait.
5. **Binary size.** Acceptable delta for `baml-cli`/packed outputs; is a
   `query` cargo feature acceptable if the gate trips?
6. **Internal profile.** Should `BAML_INTERNAL` also expose the internal
   relations, or a separate flag?
7. **Playground.** Is `/api/obs/query` in the playground server a Phase 4 goal
   for this stack or a separate effort?

---

## 11. Appendix — reuse checklist (cct-1 `32b9fe5fd` → this stack)

| cct-1 file | Action |
|---|---|
| `baml_query/src/catalog.rs` | Port types; replace v1 relation list with Section 4.2; add `Visibility`, `ViewDef`, `CatalogProfile`. |
| `baml_query/src/session.rs` | Port; add `SchemaProvider` (lazy), information_schema, views, per-execution cancellation, allow SHOW/DESCRIBE/EXPLAIN. |
| `baml_query/src/provider.rs` | Port verbatim (`RelationProviderFactory`, `PushdownClass`, `TrustedRelation`). |
| `baml_query/src/{budget,capability,outcome,error,scope}.rs` | Port; add `max_fold_bytes`; outcome unchanged. |
| `baml_query/src/value/lowering.rs` | Port incl. qualifier fix; retarget UDF bodies to `resolve_many`. |
| `baml_query/src/value/semantics.rs` | Port onto `baml_query::value::Value` (drop `CanonValue`). |
| `baml_query/src/value/resolver.rs` | Replace per-row `resolve` with `resolve_many`; keep `HydrationContext` cache + budgets. |
| `baml_query/tests/q1_gates.rs` | Port as the conformance corpus; extend per Phase 1 gates. |
| `baml_query_local/*` | Do not port; rewrite as `baml_query_profiles` over the P0 reader (`list_executions`/`ExecutionReader`). |
| `baml_cli/src/query_command.rs`, `tests/query_e2e.rs` | Port command shape, exit codes, `--schema`; make output streaming; retarget e2e to `profiles-v1`. |
| PR #4343 `hydrator.rs` `$value_ref` BFS | Not needed (CAS bodies are whole values, MVP §7.5); keep cycle/depth/bytes checks in the decoder. |
