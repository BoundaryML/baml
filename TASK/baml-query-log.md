# `baml query` implementation log

Style follows `TASK/profiling-backend-mvp-log.md`: deviations from the
canonical plan (with why), things that did not work, measured numbers behind
perf gates, and per-phase verification transcripts. Specs in precedence
order: `profiling-backend-streams.md` → `baml-query-scope.md` →
`profiling-backend-mvp.md`.

---

## Step 0 — B0 leaf crate (`bex_prof_store`)

### Deviations from the canonical plan

1. **`crossbeam-channel` added to the B0 dependency allowlist.** The scope
   doc §7 B0 row lists `sha2, hex, fs2, rustc-hash, smallvec, base64, uuid`.
   `session.rs` owns the bounded `DecoderCommand` producer lane
   (`crossbeam_channel::bounded`, `session.rs:97-99,272`), which P0 keeps
   (streams §5.5 removes it from *admission* only). Amended the scope doc §7
   B0 row in place.
2. **`prof/config.rs` moved too.** `session.rs:340` reads
   `crate::prof::ProfConfig::global()`. `config.rs` is std-only (85 lines,
   `OnceLock`), so it moves into the leaf crate instead of a hook. Amended
   the scope doc B0 row.
3. **Transport hooks.** Three call sites in the moved set reach the ring
   transport that stays in `bex_events` (`session.rs:672` consumer
   force-wake, `boundary.rs:335` `wake_for_backend_terminal`, `runtime.rs:49`
   `configure_global_transport`). New
   `bex_prof_store::prof::backend::hooks::{TransportHooks,
   install_transport_hooks}` (function pointers, `OnceLock`, uninstalled =
   no-op — wakes are advisory, the consumer's 50 ms timed park bounds the
   latency). `bex_events::prof::backend` is now a shim module: glob
   re-export + a local `register_engine_session` that installs the hooks
   before delegating — the one choke point every profiled engine passes
   before backend work can need a wake. Under `baml_loom`/wasm the hook
   install is compiled out, matching the previous cfg'd call sites.

### Verification (2026-08-24)

- `cargo check --workspace --all-targets` — green.
- `cargo tree -p bex_prof_store` — only base64, crossbeam-channel, fs2, hex,
  rustc-hash, sha2, smallvec, uuid (+ tempfile dev). No VM/tokio/engine deps.
- `cargo test -p bex_prof_store --lib` — 87 passed (the moved backend unit
  tests).
- `cargo test -p bex_events --lib` — 46 passed.
- `cargo test -p bex_engine --test profiling_backend -- --test-threads=1` —
  4 passed.
- `cargo test -p bex_engine --test identity` — 19 passed (unchanged).
- loom suite / wasm check / clippy: see below (run after the move).

---

## Step 1 — P0: thread-rooted executions and process streams

Implemented per `profiling-backend-streams.md` (formats §4, store §5.1,
writer §5.2-5.4, admission §5.5, finalization §5.6, fork guard §5.8, reader
§6, engine/session changes §7, ProgramId hash §2.3). State as of this entry:
core implementation and primary test rewrites done; remaining: gate-test
battery (§9 writer/reader unit gates not yet all pinned), MVP §11 doc
amendments, loom re-run on the P0 tree, e2e run.py + bench perf numbers.

### Deviations from the canonical plan

4. **`ProfilerConfig` grew `publish_interval` and `stream: Option<ProcessEuid>`.**
   The spec adds `publish_interval` (§5.3); `stream` is the test affordance
   §3 requires ("tests that simulate several processes pass distinct
   ProcessEuids") — `None` = `ProcessEuid::current()`.
5. **`baml_version` added to the leaf-crate allowlist.** `StreamStarted.
   baml_version = baml_version::CANONICAL_VERSION` (§4.3) and the program
   hash formula (§2.3) both need it; the crate is a 7-line, dependency-free
   constants leaf.
6. **`ProfilerStore::pending_indeterminate_token()` added** (not in §5.1's
   API listing): §7.2 says an indeterminate function-table CAS token "is
   parked in the store and picked up by the writer's step 1" — the writer
   needs an accessor to pick up a token it did not mint.
7. **`SegmentReadError::EuidMismatch` names the "euid = directory" decoder
   check** (§4.3/§4.4 mandate the check but name no error).
8. **Program hash computed in `baml_compiler2_emit` wrappers**, not in the
   `bex_project`/pack call sites §2.3 lists: every compile funnels through
   `generate_impl`, and the wrappers see the project file set, so stamping
   `Program.source_content_hash` there covers CLI, pack, LSP, and test
   compiles in one place. Mounted-unit links deliberately get `None`
   (consumer files alone under-identify the linked deps) → random
   `ProgramId`, the safe over-splitting direction. `bex_vm_types::Program`
   gained a trailing borsh field.
9. **Frozen-layout fixture updated**: `size_of::<RootProfiler>()` 17 → 40
   (`ActiveRootProfiler` now carries the 32-byte root `ThreadRef` instead of
   the 16-byte token, §5.5 step 7).
10. **e2e verifier's "exactly 3 segment files" is env-gated**
    (`PROFILING_E2E_EXPECT_MINIMAL_SEGMENTS`): with the default 1 s
    `publish_interval`, a packed run that outlives the interval legitimately
    publishes more segments; the deterministic exactly-3 shape is pinned in
    `profiling_backend.rs` (`publish_interval = MAX` + flush) instead.
11. **`OPEN_STREAMS` is keyed by euid only** (as §5.1 specifies), so two
    stores in one process with the same euid but different roots conflict;
    engine tests that share the process euid retry session creation while
    the previous test's store drops.

### Landed structure

- `store.rs`: streams layout, `publish_meta`/`publish_data`,
  `StreamHighWater`, `Blocked` vs `Indeterminate{seq}` (pitfall 2),
  `stream.lock` + `OPEN_STREAMS`, open-scan (fail-closed corrupt tail),
  `SCHEMA_VERSION=2` / `CAS_FORMAT_VERSION=1` split, `sync_file` platform
  hook, meta/data segment codecs with every §4.3/§4.4 typed error.
- `writer.rs` (new): `StreamWriter` with the §5.3 cycle, health sink,
  `exec_index`, merge-on-hand-off, meta-queue reservation (Owner::Writer),
  process-global `meta_batch_lost`/`root_ended_lost`/
  `function_table_publish_failed` counters.
- `execution.rs`: `admitted_pending` under the metadata mutex,
  `engines_started` vector, `take_admitted`, `closing_ticks`, phases
  `Open/RootReturned/Closing/Released`.
- `session.rs`: §5.5 admission (no store call/lock/I/O; fork guard;
  indeterminate gate), `engine_started`, `publish_function_table`,
  `maintain_ready_executions` (take_admitted → finalize → publish_if_due),
  `force_publish`, Stream/Execution checkpoints,
  `configure_global_store_root` + `BAML_PROFILE_DIR`.
- `decoder.rs`: `ExecutionRuntime` (root/runtime_id/program_id), hand-off to
  the writer (no publisher, no FinalizationState), durable
  `ThreadStart`/`ThreadEnd` emission with ts/name retention through pending
  tables, `apply_batch_outcomes`, §5.6 finalization (slot released
  immediately, RootEnded enqueued infallibly).
- `evidence(_codec).rs`: `SpanStart`/`ErrorCapture` lose `boundary_id`;
  `ContextRef::Overflow{reason, edge}`; `BoundaryRef` deleted; tags 6/7
  `ThreadStart`/`ThreadEnd`. Goldens regenerated deliberately (ErrorCapture
  golden verified byte-for-byte = old minus the 16-byte token; evidence
  payload golden re-pinned after adding thread facts to the fixture;
  cct_codec golden unchanged as the spec predicted).
- `function_table.rs` (new): CAS codec 2 with golden.
- `reader.rs`: full §6 rewrite (`StreamReader`/`list_executions`/
  `ExecutionReader`/`ExecutionProfile`, §6.2 status/index-state, §6.3
  severity rules, §6.4 liveness with same-process short-circuit,
  `orphan_groups`).
- Engine: `register_root(intent, root, program_id)`; activation publishes
  `FunctionTableV1` + `EngineStarted`; `build_program_metadata` consumes
  `Program.source_content_hash` (§2.3, golden
  `e98260b82b3b024bcc7d3b56fee3632a840cc8b05abf9ebcbd28882b16ef3049` for the
  formula at version 0.17.0; every variable-length field is length-framed so
  the version cannot be absorbed into the first path).
- CLI: project load configures the global store root (§7.5);
  `flush_and_join(5 s)` before `main.rs`'s exit and both
  `run_command.rs` `process::exit` sites.
- Consumer: Flush/EngineClosed publish via `flush_sessions`; park timeout =
  `min(WAKE_INTERVAL, publish_interval)`.
- LSP ignore-pattern string updated to a streams path (behaviour unchanged).

### Verification so far

- `cargo check --workspace --all-targets` green; wasm check green
  (`bex_events`, `bex_engine`, `bridge_wasm`); clippy `--workspace
  --all-targets --all-features -- -D warnings` green; `cargo fmt --check` +
  `git diff --check` green.
- `bex_prof_store --lib`: 94 passed (store/meta/data codecs, writer-adjacent
  session ports incl. the two §9-mandated ports asserting on
  `ExecutionProfile`, function table, hash formula, registry).
- `bex_engine --test profiling_backend -- --test-threads=1`: 4 passed —
  including the §9 shape gate (exactly `meta,data,meta` = 3 segment files;
  ThreadStart/End ×2; RootEnded{1,1,1,flags=0}; function table readable;
  missing-data-segment damage is a typed `DataIssue`, not an error).
- `bex_engine --test identity`: 19 passed UNCHANGED.

### Gate battery (streams §9) — status

Pinned and green (109 `bex_prof_store --lib` tests, 5 `profiling_backend`
engine tests):

- Formats: meta/data segment cross-platform SHA-256 goldens; function-table
  golden; program-hash golden; every truncation/trailing/duplicate-group/
  order/record-count violation typed (per-cut loops); `StreamInUse` on double
  open; sequential re-open resumes; distinct streams coexist; `Lost` does not
  consume a sequence; indeterminate blocks both planes + CAS until exact
  resolution; terminal one-attempt under a latched gate; corrupt tail fails
  re-open closed.
- Writer: admission-no-store-I/O with a panicking platform + p99 < 20 µs over
  10k roots (measured ~µs level); 1,000 roots → meta segments = 2, data
  files ≤ 2 (O(bytes)); age trigger publishes without flush; meta-pre Lost →
  `meta_batch_lost` + `RootEnded` flags bit 0 + reader `RootStartedLost`;
  post-rename indeterminate applied exactly once on resolution + `Blocked`
  batch published once at the next sequence (no duplicates, correct final
  ranges); re-open emits no second `StreamStarted`; indeterminate store
  rejects admission without growing `pending_meta` and the writer resolves
  the parked foreign (CAS) token; two hand-offs merge into one group with
  summed counters; `RootEnded` waits for its groups and records
  `first=1,last=3,count=3` over three cycles; slot release before any
  publication (3× slot-capacity churn with zero publications); fork child
  `Inactive(ForkedProcess)` (unix, real `fork()`); publication cost = 4
  fsyncs per segment through the platform hooks (open = 1 file + 3 dir
  syncs: usage ledger + root + the two open-scan plane syncs — the spec's
  "2 (open)" did not count the §5.1 open-scan dir syncs, which are
  mandatory).
- Reader: listing works with the whole `data/` plane deleted (never opened);
  liveness (in-process short-circuit alive/Running; after drop dead/
  Abandoned); wall clock `started_unix_ns - zero_unix_ns == started_ns`;
  `orphan_groups` finds an execution whose meta batches were all lost;
  missing data segment → `MissingDataSegment` issue (engine test); corrupt →
  `CorruptDataSegment` issue.
- Engine: program identity — byte-identical builds share `program_id` and
  root `ContextKey`s; one comment byte splits both.

Not yet pinned (tracked):
- The data-`Lost`-between-rollovers SpanState injection port (apply-outcome
  exactly-once is covered by the indeterminate/blocked test; the
  `StartUncommitted` span accounting path is exercised only indirectly).
- The 200-case `baml test` bench addition (≤ 3 segment files with
  `BAML_PROFILE_PUBLISH_INTERVAL_MS=60000`).
- e2e `run.py` + `profiling_overhead` release runs (perf numbers).
- Packed 36-task production-policy stress re-run.

### Corpus soak (3313-test `baml test`) — three consumer defects (2026-08-24)

Running the full `baml_tests` corpus through the CLI produced a broken store
(one `Abandoned` execution, one meta segment, no data) and, in some runs,
SIGABRT at exit. Subset runs (5, 329 tests) were always clean. Root causes,
in the order they were unmasked (each fix exposed the next):

1. **Consumer stack overflow (SIGABRT).** `consume_call_start` ended by
   calling `resolve_starts_for_thread`, whose loop called
   `consume_call_start` for each ready parked start — one stack frame per
   parked *sibling*, bounded only by ring content. The corpus parks
   thousands of same-thread starts (test threads outrun their spawn
   records), and the chain overflowed the consumer thread
   (`decoder.rs`). Fix: `consume_call_start_open` (park/open only) +
   two-phase iterative `resolve_starts_for_thread` — open every ready start
   first (a parked end can never make a start ready), then run the parked
   ends newest-first, preserving "children resolve before the parent's end
   strips the context key". Regression:
   `session::tests::parked_sibling_chain_resolves_iteratively_on_a_small_stack`
   (6 000-call parked chain resolved on a 512 KiB thread).
2. **Unbounded `Ring::drain` segment chase.** With producers outpacing the
   fsync-heavy decode, `drain` kept following freshly linked segments and
   one `sweep` ran for the whole test phase — control messages
   (flush/engine-closed), `take_admitted`, and publication all starved, and
   the 5 s exit flush timed out (the empty-store symptom). Fix:
   `MAX_SEGMENTS_PER_DRAIN = 16` per call; `drain` now returns
   `DrainOutcome { progress, caught_up }` and `Registry::sweep` pools an
   orphaned ring only on `caught_up` (a bound-stopped orphan stays
   `Orphaned` for the next sweep). Regression:
   `concurrency_tests::stress::bounded_drain_spreads_backlog_and_defers_orphan_pooling`.
3. **Quadratic parked-start scan.** `pending_starts` was one flat
   `HashMap<CallRef, _>` and the resolve loop scanned *all* of it per opened
   call (`min_by_key`); at the corpus's 22 k parked starts the consumer went
   CPU-bound in the scan (sampled: 87 % of consumer time) and the backlog
   death-spiraled. Fix: `pending_starts:
   HashMap<ThreadRef, BTreeMap<call_id, PendingCallStart>>` — selection is
   now "first dependency-ready in call-id order within the owning thread",
   the same choice the old filter+min made, without cross-thread scans.

After the fixes: corpus run exits 0; store has 177 executions (the harness
root plus the process/http-style per-test roots), every one
`index=Complete`, 176 `Succeeded` + 1 `Failed` (a corpus test whose root
legitimately errors), `hw meta:7/data:4`, no index gaps; the exit flush
completes inside the 5 s budget.

Also landed with this investigation:

- `crates/baml_tests/tests/baml_src.rs` now sets `BAML_PROFILE_DIR` into the
  test tempdir — the corpus run otherwise writes
  `crates/baml_tests/baml_src/.baml/profiles-v1` into the source tree that
  sibling tests scan. (Polluted tree from earlier runs deleted.)
- `segment_growth_pressure_rejects_record_without_aborting` (pre-existing,
  from the earlier bound-consumer-work commit) was not gated
  `#[cfg(not(baml_loom))]` and aborted the loom suite; gated like the rest
  of the real-thread tests. Both loom suites green
  (`bex_events` 41, `bex_prof_store` 110).
- All temporary debug instrumentation from the investigation removed
  (`prof_debug` and friends never existed in the committed tree).
- wasm `cargo check -p bex_prof_store` warning-clean again (cfg gates for
  the hooks fns and wasm-dead session items).

### Step 2 note — DataFusion pin moved to `=54.1.0`

The scope doc pinned DataFusion `=55.0.0` (cct-1's version), but 55.0.0
declares `rust-version = 1.94` and the workspace toolchain is pinned to
`1.93.0` (`rust-toolchain.toml`) — 55 does not build here at all. 54.3/54.2
do not exist on crates.io; `=54.1.0` builds clean on 1.93. Chose the
dependency downgrade over a workspace-wide toolchain bump (the less invasive
default); the cct-1 port adapts to the 54 API where they differ. Scope doc
§7 amended.

## Step 2 — Phase 1: `baml_query` core (cct-1 port)

Landed (all against catalog v1 from the scope doc §4, not cct-1's canon
catalog):

- `catalog.rs`: new declaration model (`Visibility`, `ViewDef`,
  `CatalogProfile` + `Override`), the nine v1 relations
  (threads/contexts/calls/errors/functions/health public;
  streams/segments/cas_objects internal), the two shipped views
  (`llm_calls`, `hot_contexts`), column golden + profile-gating tests, and
  the §4.2 "no executions view" decision pinned by a test.
- `error/outcome/scope/budget/capability/provider`: near-verbatim cct-1
  ports (+ `max_fold_bytes` budget, + `did_you_mean` Jaro-Winkler remedy
  helper per §5.7).
- `value/semantics.rs`: ported onto the neutral `Value` model (Class by
  `name`, `Presence` tri-state, `MediaContent`); `to_json` defined here
  (canonical JSON projection: classes render present fields, enums their
  variant, bytes as `{"$bytes": hex}`, media/omitted as tagged objects).
- `value/resolver.rs`: the trait is now **batched** (`resolve_many`) per
  §5.5; `HydrationContext::resolve_batch` dedupes within the Arrow batch,
  budgets misses, caches hits AND misses, and calls the resolver once.
- `value/lowering.rs`: ported; roles are `input|output|error` (the
  args-root numeric-subscript error keys on role `input`); local
  `parse_cid_wire` (`bamlv_1_<hex64>`) replaces the old store dependency;
  `VcmpCidUdf` restructured so the CID shortcut answers rows without
  hydration and the rest resolve in one batch.
- `session.rs`: `QuerySessionBuilder::build` is now **async** (views are
  planned at build); lazy `BamlSchemaProvider` under catalog
  `baml`/schema `public` with `information_schema` (§5.2); gatekeeper
  extended to allow `SHOW TABLES`/`SHOW COLUMNS`/`DESCRIBE`/`EXPLAIN
  <query>` (§5.8); `plan_error` unwraps `Context`/`Diagnostic` wrappers,
  restores provider-typed errors from `External`, and attaches a
  did-you-mean remedy on unknown tables.
- `tests/q1_gates.rs`: full cct-1 gate suite ported (18 gates) onto
  `calls_v1` (Utf8 wire ids), plus new gates: views plan + discovery
  statements execute, unknown-table remedy, alias set
  (`calls`/`retained_calls`), and the dependency allowlist now FORBIDS
  `bex_events` (backend-neutral core per §3.1).

Verification: 17 lib + 18 gate tests green; `cargo clippy -p baml_query
--all-targets` warning-free; `cargo tree -i datafusion` reaches only
`baml_query`.

### P0 gate battery — remaining items closed (2026-08-24)

- Release e2e (`crates/baml_tests/profiling_e2e/run.py`): baseline
  relative slowdown **1.116×**, packed 36-task stress **1.051×**, both
  scenarios `passed`, verifier green (1 stream / 2 executions / Complete).
- `profiling_overhead` bench (divan, release): pure_call_1m on/off
  medians 113.8 ms / 101.3 ms (**+12.3 %** on the 1 M-structural-record
  microbench), spawn_await_x10k +5.9 %, one_wait_per_call ±0.4 %,
  `prof_suppressed` ≡ off (101.5 vs 101.3 ms — suppression is free).
- Segment batching gate, run at full-corpus scale instead of 200 cases:
  the 3313-test corpus with `BAML_PROFILE_PUBLISH_INTERVAL_MS=60000`
  produces **5 segment files** (3 meta + 2 data; data split only by the
  segment byte target) — no per-test segments.
- Still tracked, not pinned: the data-`Lost`-between-rollovers SpanState
  injection port (apply-outcome exactly-once is covered by the
  indeterminate/blocked tests).

## Step 3 — Phase 2: `baml_query_profiles`

New crate (deps: `baml_query`, `bex_prof_store`, datafusion, prost,
num-bigint, base64, hex, sha2 — never `bex_engine`/`bex_vm`):

- `universe.rs` — §5.6 bind: every stream's meta plane once
  (`StreamReader::open`), frozen summaries, `generation =
  sha256("baml-query-generation-v1" ‖ sorted (stream_id, meta_hw,
  data_hw))`, `projected_through` = max committed data sequence.
- `fold.rs` — §5.4 fold cache: `ExecutionReader::load()` once per
  (execution, data range, ended?) behind an entry-approximated LRU
  bounded by `max_fold_bytes`; function tables cached by CAS cid.
- `decode.rs` — **B4**: a private prost mirror of the codec-1
  `BamlOutboundValue` subset the engine's trace encoder emits (tags
  2–20) → neutral `Value`; wire-hex bigints → minimal decimal;
  `baml.trace.OmittedValue` classes → `Value::Omitted`; depth cap →
  typed `Truncated`. No dependency on `bridge_ctypes` needed.
- `resolver.rs` — handle wire `0x01‖u16 codec‖cid32` / `0x00‖reason`;
  batched `resolve_many` reads the CAS (`decode_cas_object`, cid
  verified); `canonical_cid` is exact for codec-1 handles.
- `relations.rs` — one provider per relation; a scan prunes executions
  by pushed-down `execution_id` equality/IN (classified `Exact`), folds
  the survivors, and materializes catalog-shaped batches (delegating
  projection/limit to a MemTable).

Deviations from the scope doc (logged, all conservative):
1. Physical plan is materialize-then-MemTable, not the §5.3 streaming
   `ProfilesScanExec` with per-chunk partitions — correct, simpler,
   revisit when stores outgrow memory.
2. Pushdown classes: only `execution_id` equality/IN is `Exact`;
   `status`/`stream_id` exactness and the `InexactCandidate` time/fqn
   pruning are deferred (everything else `Unsupported`, so DataFusion
   re-filters — semantics identical, less pruning).
3. `error_id` wire form is `<thread wire>#<unwind_ordinal>` (no spec'd
   encoding exists for `ErrorCaptureId`).
4. `segments_v1.checksum_ok` equals `decode_ok` (the decoder validates
   checksums internally; no separate probe).
5. Threads rows always fold (§5.4's "threads-only queries never touch
   data segments" holds only for the meta-derived execution columns; the
   non-root rows need the durable thread facts, which live in the data
   plane). A meta-only fast path is a later optimization.

Verification: 4 decode unit tests; `tests/profiles_e2e.rs` writes a real
store through the producer session (register_engine_session →
consume_engine_bytes → maintain/flush) and reads it back through SQL
(threads/calls/contexts/health rows, SHOW TABLES, deterministic
generation across binds) plus CAS handle resolution (codec-1 decode +
canonical-cid shortcut + typed unavailability). Clippy warning-free.

## Step 4 — Phase 3: CLI `baml query`

- `crates/baml_cli/src/query_command.rs`, wired as `Commands::Query`
  (project resolution via `find_project_root_from`, same as `clean`;
  global `--project` applies).
- §6 contract: SQL positional or `-` (stdin); `--schema [--table NAME]`
  renders the profile (JSON shape per spec incl. views; table format for
  humans); `--format table|json|jsonl`; `--explain`; `--max-rows`;
  `--max-wall` (`30s`/`1500ms`/seconds); `--internal` or
  `BAML_INTERNAL=1` → `CatalogProfile::internal()`.
- Streaming: `jsonl` rows leave per batch with a terminal
  `{"queryOutcome": …}` frame; `table` freezes widths on the first batch
  and streams (60-char cell truncation with `…`); `json` = one
  `{version, rows, queryOutcome}` envelope. Outcome to stderr in table
  mode.
- Exit codes (§6): 0 complete · 1 incomplete · 2 invalid SQL/unknown
  table/authorization · 3 budget · 4 cancelled · 5 dependency/internal.
  Deviation: Ctrl-C exits through the CLI's global handler (130), the
  cancel token stays host-driven — exit 4 is reachable via library
  cancellation only.
- §5.8 `baml_columns`: column docs are SELECT-able
  (`SELECT doc FROM baml_columns WHERE relation='calls_v1' AND
  "column"='args'`), registered at session build from the profile; gated
  in q1.
- Docs per the agent-native rule: clap doc comments + examples in
  `after_long_help`, and a `query` topic in
  `baml_builtins2/keyword_docs/baml_keywords.yaml` (`baml describe
  query`). Nothing under `fern/`.
- Gates: `crates/baml_cli/tests/query_e2e.rs` — a real `baml run` then:
  executions idiom returns `succeeded`/`user.main` with
  `result=complete` on stderr, exit 0; jsonl = 2 rows + outcome frame;
  unknown table → exit 2 with `did you mean \`threads\`?`; DML → exit 2;
  `--schema --table calls --format json` renders; `--max-rows 1` → exit
  3; no store → exit 5 with the run-something-first remedy.
- Manual e2e: `baml run` + value queries hydrate real captures
  (`values=2/2 available`, output column renders bare scalars, absent
  path = complete non-match).

Phase 4 (playground `/api/obs/query`) not started — out of scope per the
handoff (stop after Phase 3).

### Post-Phase-3 fix: `COUNT(*)` (2026-08-24)

Building the showcase demos surfaced that every `COUNT(*)` failed with
"Physical plan does not support logical expression Wildcard".
`SessionStateBuilder::with_expr_planners` REPLACES the default expr
planners, so registering `BamlValuePlanner` had silently dropped
DataFusion's aggregate planner (the `COUNT(*)` → `count(1)` rewrite).
The session now prepends our planner to
`SessionStateDefaults::default_expr_planners()` instead; gated by
`count_star_plans_with_the_default_planners_intact` (q1 suite → 20
gates). Also noted for docs: a view's inner `ORDER BY` does not survive
an outer query (standard SQL), so `hot_contexts` consumers should order
explicitly.

### Backend-truth sweep + planner-defaults hardening (2026-08-24)

Removed what no backend can produce, implemented the one dormant column
pair that has backend data, and widened the gate that would have caught
the `COUNT(*)` bug class:

- **`llm_calls` view removed from catalog v1** (scope §4.2 amended).
  `FunctionTableV1` kind codes are
  `bytecode|sysop|native|native_unresolved` — no producer emits an LLM
  kind, so `WHERE kind = 'llm'` could only ever return an empty,
  misleading result. The gate now asserts the view does NOT resolve;
  CLI examples, the `baml describe query` topic, and the demo reference
  were updated. Re-adding it when the kind lands is additive (stays v1).
- **Catalog docs corrected to the actual code sets**: `kind` docs no
  longer claim `llm`; `origin` docs now read
  `user|companion|internal|builtin|auto_derive` (was the imagined
  `user|stdlib|generated|internal`).
- **`threads.spawn_function_id` / `spawn_fqn` implemented** (were
  unconditionally NULL): resolved through the spawning call's retained
  span (`ThreadStart.spawn_call` → `SpanStart.function_id` → function
  table). Verified live: named shard workers report
  `spawn_fqn = user.main`.
- Kept `revision_id`/`source_label`: their plumbing exists end to end
  (program metadata → `EngineStarted`); the values are currently `None`
  at the emit sites, which is an upstream-population gap, not a missing
  backend.
- **New q1 gate `standard_sql_surface_survives_the_custom_session_assembly`**
  (suite → 21 gates): the `COUNT(*)` regression's bug class is session
  assembly that silently drops DataFusion defaults, so the gate runs a
  battery through every default-owned planning path — `COUNT(*)` (bare,
  over a view, grouped, joined), `COUNT(DISTINCT)`, CTEs, scalar
  subqueries, `UNION ALL`, window functions, `LIKE`/`CASE`/scalar
  functions, subscripts on a RESIDENT list column (must fall through our
  field-access planner to the default one), and `information_schema` —
  each must plan AND stream to a complete outcome.

### Link-oracle regression: `source_content_hash` redesigned as
### in-memory metadata (2026-08-24)

The final unfiltered workspace run caught `link_units_oracle` red (5
byte-identity tests): the flat compile stamps
`Program.source_content_hash` but `link(emit_units(p))` reproduced the
program without it — a 33-byte borsh tail difference. (It had been red
since the stamping landed; the earlier suite log was piped through
`tail -80`, which truncated the failures — verification-process lesson
recorded: capture filtered failures unbounded, never `tail` them.)

First fix attempt — carrying the hash on a `CompilationUnit` and
restoring it in `link()` — was rejected by the next gate down:
`mounted_package_parity::emitted_program_dependency_and_consumer_units_are_byte_identical`
requires the SAME source file's unit to be byte-identical whether its
dependencies were compiled from source or mounted as a blob, and a
project-wide hash on any per-file unit breaks that by construction. The
carrier was reverted.

Final design: **the hash is not compiled content.**
`Program.source_content_hash` is now `#[borsh(skip)]` — in-memory
metadata attached at program-materialization boundaries:

- fresh compiles: stamped by the `generate_project_bytecode_*` wrappers
  (unchanged);
- CLI bytecode-cache hits: `ProjectSession::try_cached_program` restamps
  via the now-`pub` `project_source_content_hash(db)` — the cache hit
  just validated every project file byte-for-byte against that database,
  so the recomputed hash is exactly what a fresh compile would stamp
  (verified end-to-end: cold run + warm cache-hit run share one
  `program_id` in `baml query`);
- packed binaries (`baml pack` host): no sources at runtime → `None` →
  random per-engine `ProgramId` (the safe, over-splitting direction).
  Known gap, tracked: embed the hash in the pack manifest at pack time.

Every byte-identity oracle now holds naturally (link, relink, parity,
determinism, `compiled_package_identity`, `runtime_package_*` — all
green), `CompilationUnit` stays free of project-wide state, and engine
identity tests (19) + profiling tests (5) still pass.

### Workspace-suite verification hygiene (2026-08-24)

Two machine/process issues were masquerading as regressions in workspace
runs and are now resolved and understood:

- `sdk_test_cpp` (all 12 fixtures): CMake configure failed on a missing
  `target/cpp-protobuf-src` — provisioned by the nextest setup script
  (`sdk_tests/crates/cpp/setup.sh`), which plain `cargo test` never
  runs. After one `bash sdk_tests/crates/cpp/setup.sh`, all 12 pass.
  Same class as the `baml_bridge` failures earlier (prebuilt
  `bridge_cffi` dylib, also a nextest-setup product).
- Process lesson #2 (after "never `tail` a filtered failure log"):
  piping `cargo test --workspace` through `grep` makes `$?` the GREP
  exit, not cargo's — two earlier "green" workspace verdicts were
  actually the filter's exit status. The final gate now writes full
  unfiltered output to a file and records cargo's own exit code.

### Four-dimension workflow review of 39da417ad (2026-08-24)

22-agent workflow (4 dimension reviewers → adversarial verifier per
finding; two verifiers ran live repro tests): **17 confirmed, 1
refuted**. Fixed in the follow-up commit:

- **HIGH — authorization/capability gates skipped subquery plans**
  (`session.rs`): the plain `apply` family never enters plans embedded
  in `EXISTS`/`IN`/scalar subqueries, so a no-value-read scope could
  hydrate values (and gated functions could run) from inside any
  subquery — verified with a live repro. Both walks now share
  `scan_scalar_functions` over `apply_with_subqueries`; gated by
  `value_reads_inside_subqueries_are_denied_without_the_right`.
- **HIGH — hydration answers read back through the evictable LRU**
  (`resolver.rs`): a batch with more distinct handles than the 4096-entry
  cache evicted fresh resolutions before the answer pass and mislabeled
  rows `query_budget_exhausted`. Answers now come from a per-batch map;
  the LRU is opportunistic dedup only.
- **MED — bare-value render rewrite corrupted interior handles**
  (`lowering.rs`): the rewrite ran over every projection, handing
  `__baml_vcmp` rendered text under derived tables/CTEs (live-repro'd
  "value handle column must be Binary"). Now rewrites only the output
  chain (root projection through sort/limit/distinct/union/aliases);
  gated by `derived_table_value_columns_stay_handles_for_outer_predicates`.
- **MED — Exact execution_id pushdown widened after an empty
  intersection** (`relations.rs`): `Some(vec![])` was indistinguishable
  from "no filter yet", so contradictory equalities returned rows.
  Proper Option sentinel; gated by
  `contradictory_execution_id_equalities_return_zero_rows`.
- **MED — reader ignored BAML_PROFILE_DIR**: store-root resolution now
  lives in ONE place (`ProfilerSession::resolve_store_root`, env wins,
  else `<project>/.baml/profiles-v1`) used by producer configuration and
  `baml query`; verified round-trip through a custom store dir.
- **MED — errors_v1 terminal ids were O(errors × spans)**: one-pass
  reverse index per execution.
- **MED — builder/catalog drift was silent**: `BatchBuilder::push` now
  debug-asserts no row cell was left unmatched by a catalog column.
- **MED — catalog goldens froze full type+nullability only for
  threads_v1**: all nine relations now render-frozen.
- **MED — unknown codec-1 variants decoded as silent SQL NULL**: a
  `None` oneof (a NEWER engine's tag) now decodes as
  `Omitted("unknown codec-1 value variant")` — loud, typed drift.
- **LOW — reserved-prefix gate rejected `'__baml_'` inside string
  literals**: the scan now ignores single-quoted spans; gated both ways.
- **LOW — `__baml_` convention had no shared constant**:
  `lowering::INTERNAL_FN_PREFIX` now feeds the gatekeeper, the
  authorization walk, and `TrustedRelation`'s filter guard.
- **LOW — loss-byte table duplicated `ValueLossReason` numbering**: one
  `LOSS_TABLE` drives encode and decode, with a round-trip test.
- **LOW — provider over-exposed storage internals**: `universe`/`fold`/
  `relations`/`decode` are now crate-private; the public surface is the
  session constructors + `ProfilesResolver` (the Phase-4 endpoint must
  come through the session seam).

Deferred, tracked (all LOW, verified-real but bounded today):
- Row-builder per-row HashMap cost (folds into the already-tracked
  streaming-scan-exec revisit; the same rework replaces the Row model).
- Consumer parks 50ms between bound-stopped drains (bounded backlog
  latency ~N/32×50ms; propagate `caught_up` through `service_once` when
  it bites).
- FoldCache can double-fold under concurrent same-execution scans
  (single-query CLI today; add an in-flight guard with the Phase-4
  multi-session embedding).
- `query_command` exits via `process::exit` like `run_command`'s target
  paths rather than the `ExitCode` enum (§6 codes don't fit the enum;
  unify when a second consumer of custom codes appears).

Refuted (1): "ProfilesResolver re-implements the CAS layout" — the raw
path read is the deliberate B4 design (the resolver has no execution in
hand; layout is spec'd store contract, verified against `decode_cas_object`).

## 2026-08-24 — catalog rename (team review, Slack thread w/ Vaibhav + Aaron)

Agreed renames, applied across catalog, provider, CLI, gates, keyword docs,
TASK docs, and the demo-query reference docs. Physical store layout
(`streams/` dir, segment file format, `stream.lock`) unchanged — this is a
virtual-surface rename only; Rust-internal identifiers (ContextKey,
stream refs) intentionally keep the implementation vocabulary.

| was | now |
|---|---|
| `contexts` / `contexts_v1` | `call_path_stats` / `call_path_stats_v1` |
| `context_id` / `parent_context_id` / `throw_context_id` / `context_overflow_reason` | `call_path_id` / `parent_call_path_id` / `throw_call_path_id` / `call_path_overflow_reason` |
| `hot_contexts` | `hot_call_paths` |
| `functions` | `function_definitions` |
| `streams` | `processes` (key `stream_id` → `process_id`, `pid` → `os_pid`; also `threads.stream_id` → `process_id`) |
| `segments` | `store_files` (`threads.data_segment_count` → `data_file_count`) |
| `cas_objects` | `value_index` |
| health plane value `stream` | `process`; metrics `*_segment_publish_failed` → `*_file_publish_failed`; overflow reasons `context_memory_unavailable` → `call_path_memory_unavailable`, `invalid_parent_context` → `invalid_parent_call_path` |

Rejected from Aaron's list with rationale: `segments → call_path_stats_deltas`
(segments hold all evidence planes, not just CCT deltas — renamed to
`store_files` instead); `cas_objects → function_value_index` (the store also
holds captured values and error values — `value_index` instead).

Hardening found during the rename: the provider's relation-name → builder
dispatch was a bare match that only failed at runtime ("no provider for
relation …"). Extracted `provider_for()` and added
`provider_coverage::every_catalog_relation_has_a_row_builder`, which walks
`CatalogProfile::internal().relations()` — a future rename on either side now
fails at test time.

Verification: baml_query + baml_query_profiles + baml_cli suites green
(incl. q1 gates, profiles e2e, query_e2e); all demo-query documented queries
re-run verbatim against the pristine demo stores; old names fail loudly
(`invalid_sql: table … not found`).
