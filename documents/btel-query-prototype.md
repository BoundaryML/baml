# BTEL query prototype: implementation plan

Status: stages 1 to 5 are implemented and tested in this worktree. Stage 6
measurements are done. The reader phase authorized in
[BTEL query expansion](btel-query-next-phase.md) is implemented and tested:
threads, calling contexts, recursion-aware statistics, richer retained calls,
function metadata, clocks, capability discovery and the playground views.
See [Reader phase](#reader-phase-2026-09-24), the
[playground follow-up](#playground-follow-up-2026-09-24) and the runnable
[demo](btel-query-demo.md). The subsequent [short reader benchmark and child-time
cache](btel-query-performance.md) are implemented. The current
[population-outcome phase](btel-query-outcomes.md) preserves success/error/cancellation
counts in background aggregation and exposes them in SQL and the playground
calling-context inspector. It intentionally changes the processor/recorder;
the VM, event records and CAS remain frozen.
Use the outcome phase's explicit manifest for the current boundary audit.
The [remaining parity review](btel-query-parity-review.md) led to
[structured value comparisons](btel-query-value-comparisons.md), now implemented
entirely in the reader. Recorded call/spawn source locations and error raises
are now implemented; see the [source/error record](btel-query-source-errors.md)
and the subsequent [origin review](btel-query-error-origin-review.md).
Further statistics optimization is deferred.
Earlier validation and measurements below describe their respective phases.

Current base: `codex/btel-cloud-delivery` / PR #4958, commit
`bf96803b9460d9b60f20e4bee66545b99aa3ba47`. PR #4986 is stacked directly on it.
The [stack update](btel-query-stack-update.md) incorporates upstream's CAS
format v2 and compiler changes while retaining our BTEL 2.2 metadata and
reader features. Older measurements below were not rerun for this merge.
The original prototype and measurements below used `codex/btel-snapshots` /
PR #4953 at `0c30d21021526ba1c2369e9e021b9d28ac1dc823`; their historical results
are not measurements of the rebased implementation. See
[Cloud-stack integration](#cloud-stack-integration-2026-09-24).
Worktree: `btel-query-prototype`; branch: `antoniosarosi/btel-query-prototype`.

## Outcome

Build a small, real path from a BAML execution to useful local queries:

```text
BAML execution -> existing BTEL files + CAS blobs
                              |
                 discover and reconcile new files
                              |
                    disposable SQLite database
                              |
SQL -> sqlparser -> BAML expression translation -> SQLite -> results
                                                    |
                                          lazy CAS value resolution
```

The first milestone is a working `baml query` command, including captured-value
brackets. A small playground integration then exercises the same library while
recording remains active. We expand the catalog after measuring this path.

Cloud delivery and Boundary Studio remain product requirements. This prototype
tests the local path; it does not implement cloud delivery or claim that the
local database is the cloud storage architecture. Recording/CAS decoding and
reconciliation must be reusable by a later cloud reader.

## Decisions already made

- Keep BTEL/CAS recording. SQLite is derived and is created by the first query
  or playground reader, never by enabling runtime telemetry alone.
- Preserve bracket navigation: `args['customer']['age']`,
  `output['items'][0]['name']`, and equivalent error-value access.
- Use `sqlparser` directly, initially the existing dependency version 0.62.0.
  SQLite performs relational planning and execution; registered Rust functions
  implement BAML value operations.
- Implement one explicitly selected project/source and multiple recordings.
  Default derived storage: `<project>/.baml/btel/query.sqlite`, beside
  `recordings/` and `cas/`. Internal identities include source and recording.
- Keep the direct-SQLite recorder experiment and its results in the local
  archive listed below. Its unused worktree was archived for removal at the
  user's request. Do not carry its engine/sink modifications into this prototype.
- Treat table names and columns below as a prototype contract, not the final
  catalog. Preserve source evidence so interpretation can change later.

The recorder benchmark motivates keeping database work out of recording. It
does not establish that SQLite is the best derived query backend, or measure
BTEL-to-SQLite indexing. This prototype supplies those missing measurements.

## What the base already provides, and what is missing

| Existing component | Reuse and required work |
| --- | --- |
| `baml_language/crates/btel_file/src/lib.rs` | Completed files are closed and renamed from `.btel.part` to `.btel`. Keep publication behavior. |
| `baml_language/crates/btel_file/src/reader.rs` | Validation and directory reading exist, but `read_directory` rereads all files and returns a fresh snapshot. Extract bounded single-file decoding/validation for incremental use; never apply whole snapshots as deltas. |
| `baml_language/crates/btel_publisher/proto/recording.proto` | Defines announcements, completions, additive aggregates, metadata, clocks, and terminal markers. Definitions can arrive after their references. |
| `baml_language/crates/btel_snapshot/src/encoding.rs` and `hash.rs` | Snapshot encoding and graph hashing exist. Add a production decoder and identity verification; the blob header alone is insufficient. |
| `baml_language/crates/baml_query/src/value/` | Reference for navigation, comparisons, missing values, and tests. Its DataFusion planner and tree-shaped value model are not drop-in implementations for the new graph format. |
| `baml_language/crates/baml_cli/src/query_command.rs` | The CLI arguments remain, but this branch's command currently reports profiling unavailable. Connect a real BTEL backend. |
| `baml_language/crates/baml_cli/src/runtime_telemetry.rs` | CLI execution already configures local BTEL recording. Use this path in integration fixtures. |

There is a concrete prerequisite for named input navigation: snapshot-v1
`FunctionArgs` records parameter slots, not parameter names, and the current
wire function metadata does not include names. Names must come from recorded
metadata, not today's source files, because programs can change after a run.

Add an optional argument-layout field to the cold function-definition metadata.
It should preserve slot positions and optional names, including receiver and
unnamed slots. The existing `Function::argument_layout()` and
`Function::runtime_metadata()` in `bex_vm_types/src/types/function.rs` provide
the extraction point. Distinguish absent metadata from a known empty layout.
Make the wire change additive and follow the existing format-version policy.
Old recordings without that metadata report named-input navigation as
unavailable; output navigation remains possible.

This is the only planned producer-side addition. It must not add fields to VM
frames, hot function objects, transport records, capture values, or ring-buffer
entries, and must not change the CAS encoding or hashing. Measure the cost of
copying/publishing the extra definition metadata rather than assuming it is
free. If the upstream stack already supplies this metadata when implementation
starts, reuse it instead.

## Small public catalog

Start with these relations. Add a column only when an example query or the
minimal playground view needs it.

| Relation | Minimum information and meaning |
| --- | --- |
| `recordings` | Source/recording identity, observed sequence, indexed trustworthy sequence, terminal sequence if seen, evidence/lifecycle state. |
| `executions` | Root-thread identity, recording identity, known entry function, observed completion status, nullable duration and timing status. A recording can contain many executions. |
| `calls` | Individually retained call identity, execution/thread/parent identity, function name, status, nullable duration, timing state, capture references, and virtual `args`, `output`, `error` columns. |
| `function_stats` | Execution/function identity, recorded aggregate count, duration and self-await totals, timing state. Counts cover the recorded aggregate population, not just retained calls. |
| `issues` | Recording/sequence, affected identity when known, diagnostic code and explanation. |

Calls and aggregate counts answer different questions. Do not infer total call
counts by counting retained spans, add retained spans to aggregate counts, or
present aggregate coverage as covering functions excluded by recording policy.
Keep duration and self-await distinct; do not label either as CPU self time.

The internal schema needs an import ledger, schema/normalization versions,
function/thread/call-path/clock definitions, announcement/completion evidence,
aggregate deltas and reduced totals, capture references, and issues. These are
implementation tables, not a promise to expose the full previous catalog.
Keep per-file provenance sufficient to rebuild an affected recording.
Start with unique import/evidence keys and indexes for recording/execution
lookup, call parentage, and execution/function statistics. Add further indexes
only when the prototype queries or their measured plans require them.

Scope all identities correctly. Preserve unsigned IDs/ticks losslessly rather
than casting them into signed SQLite integers. Use fixed-width binary internal
IDs with stable text rendering where needed; numeric sums and time conversion
must be checked and report overflow. Missing definitions and invalid clocks
yield unavailable derived fields, never invented names or zero durations.

Example acceptance queries, with provisional relation/column names:

```sh
baml query --from /path/to/project --schema
baml query --from /path/to/project \
  "SELECT recording_id, indexed_sequence, state FROM recordings"
baml query --from /path/to/project \
  "SELECT fqn, SUM(call_count) AS n FROM function_stats GROUP BY fqn ORDER BY n DESC"
baml query --from /path/to/project \
  "SELECT call_id, output['items'][0]['name'] AS name FROM calls WHERE status = 'ok'"
baml query --from /path/to/project --format json \
  "SELECT c.call_id FROM calls c WHERE c.args['customer']['age'] >= 30"
```

Return rows plus a terminal outcome containing indexed extents and evidence/CAS
diagnostics. JSON output must let an agent distinguish an empty answer from an
answer made incomplete by unavailable evidence. Keep diagnostics out of row data.

## Incremental indexing and live reads

1. Resolve `--from` once to the selected project's `.baml` source. The reader
   receives this explicit path; it performs no recursive source discovery.
   An absent recording directory produces an actionable empty-source result.
2. Open/create the derived database and verify its versions. Serialize database
   creation, migration/rebuild, and reconciliation with a process-safe writer
   lock. A second fresh query waits within its deadline, then rechecks state.
   It must not silently fall back to stale results on a lock timeout.
3. Take a finite discovery snapshot. Ignore `.part` files. Compare completed
   file identities with the ledger; metadata checks may skip unchanged files.
   Hash newly read or changed files and validate identity, sequence, and format.
4. Apply each recording's contiguous valid prefix from sequence 1. Keep files
   beyond a gap out of ordinary facts and report the gap. Metadata references
   that resolve later do not prevent importing structurally valid evidence.
5. Apply a bounded group of complete files in one SQLite transaction. Update
   facts, derived totals, issues, the import ledger, and the trustworthy
   watermark together. The ledger prevents additive deltas being counted twice.
6. Release writer ownership and execute SQL in a fixed read transaction.
   Return the actual indexed extent with the result. Files published after the
   discovery snapshot are eligible for the next refresh; never chase the writer
   indefinitely or wait for application shutdown.

Use WAL and `synchronous=NORMAL` initially for the derived database, with a
bounded busy timeout and checkpoint policy. Separate query connections allow
already-current readers to continue during indexing. Bound query duration so
long read transactions do not indefinitely hold back checkpoint progress.
[SQLite documents this concurrency model](https://www.sqlite.org/wal.html).

An announcement can become a completed call after a later import. A completion
may also appear without an earlier announcement. Reconcile by identity and
flags, including late completion, rather than requiring one event order.
Later definitions and clock observations repair affected derived fields;
conflicting metadata remains diagnosable. Retain raw timing evidence for this.

Only a terminal marker plus the required complete valid prefix justifies
sealing. No recent writes is not evidence of completion. Report recording
evidence health separately from an execution's observed outcome.

If a previously applied file changes or disappears, invalidate that recording
and rebuild it from the remaining trustworthy prefix. Do not append new facts
to the old interpretation. Schema changes rebuild derived state under writer
ownership; readers must never select a partially rebuilt schema. A crash before
commit retries the files; a crash after commit sees their ledger entries.

The normal fast path assumes published files stay immutable. Metadata-based
checks cannot detect every mutation preserving filesystem metadata. Keep an
explicit full-verification/rebuild path and document this limit; never claim
that successful protobuf decoding proves content integrity.

## SQL and captured-value implementation

Use a new native `baml_query_btel` crate for the coordinator, SQLite storage,
catalog, SQL frontend, result types, and query budgets. Keep file/CAS decoding
and semantic reconciliation in a SQL-independent `btel_reader` component.
CLI and playground depend on the query crate; engine recording never does.
These names are proposed implementation boundaries, not public API names.

The frontend uses `sqlparser` with the existing bracket-capable input grammar
and an explicitly supported SQL subset. Bind column references against the
small catalog, carrying BAML-value identity through aliases, joins, and
supported CTE/subquery projections. Translate navigation and comparisons into
internal functions and emit SQLite-compatible SQL. Preserve value handles
through intermediate projections; render them only when producing final values.
The parser is a syntax library, not an automatic SQL-dialect converter.
[sqlparser describes those boundaries](https://github.com/apache/datafusion-sqlparser-rs).

Port behavior deliberately: constant string keys, zero-based integer list
indices, named input roots, scalar projections and comparisons. Reject computed
paths, slices, and unsupported value operations with useful errors. Broader
graph equality and the old catalog are outside the prototype. Do not route BAML
comparisons through SQLite's implicit coercions when that changes their meaning.
Tests must cover a name reused in separate query scopes, not just simple strings.

Register internal functions on every query connection using rusqlite's
`functions` feature. Bind SQL parameters through SQLite's parameter API. User
SQL is read-only: enforce the statement policy and a SQLite authorizer, with
no user writes, attachment, extension loading, or arbitrary internal-function
access. Scope internal calls introduced by the translator explicitly.
[SQLite provides authorization during statement preparation](https://www.sqlite.org/c3ref/set_authorizer.html).

The CAS decoder reads the current blob version into an owned graph, validates
tags/references/lengths and recomputes the versioned graph identity. The current
ID is 16 bytes and is not a raw hash of the complete blob; do not reuse the old
32-byte value-ID assumption. Preserve shared objects, cycles, omitted slots,
truncation and scalar types. Navigation walks a bounded path through the graph;
it does not expand it into an unbounded JSON tree.

Resolve values lazily. Metadata-only queries must perform zero CAS reads.
Within one query, memoize hydration outcomes with byte/entry limits, including
missing/corrupt results. A subsequent query can retry a previously missing blob.
Report unavailable evidence separately from captured null or an absent field.
Enforce file/blob size, decoded allocation, path traversal, time, and output
budgets, including time spent inside Rust callbacks.

For the prototype, bounded whole-value rendering can use an explicit graph
envelope with reference nodes. Scalar bracket projections return useful scalar
values. Do not promise that arbitrary captured values are lossless ordinary JSON.

## Implementation sequence and completion gates

### 1. Real fixture and recording metadata

Create an offline fixture through the real engine/CLI recording path: several
root executions, repeated ordinary calls, spawned children, retained captures,
a failed call, and a run that stays alive across multiple file publications.
Use a deterministic capture-enabled test policy without external model calls.
Include repeated and distinct captured values. Add recorded argument-layout
metadata as described above and verify slot/name alignment and absent metadata.

Gate: known program results and published BTEL/CAS files; existing recording
tests still pass; no VM/GC/transport layout change.

### 2. Structural reader, SQLite index, first CLI query

Extract single-file validation, implement recording reconciliation and the
ledger, then wire the small catalog to `baml query`. Start with metadata and
aggregate queries so ingestion correctness can be checked independently of CAS.
Use the existing snapshot reader only as a validation reference for valid files.

Gate: first invocation creates the DB; a new CLI process over unchanged files
decodes zero BTEL files; adding one valid segment applies its deltas exactly
once. Publish counters for discovery, bytes read, files decoded/applied,
reconciliation, commit, SQL execution, and output rendering.

### 3. CAS decoder and bracket queries

Implement bounded graph decoding/verification, named-input mapping, SQL AST
binding/translation, and registered value functions. Reuse old navigation tests
where their semantics fit the new format; add graph-specific tests.

Gate: the bracket examples above work against real captured inputs/outputs in
fresh CLI processes, including aliases and a CTE. Missing, corrupt, omitted,
and cyclic values produce defined results and diagnostics. Metadata queries do
not hydrate CAS, and repeated paths share hydration within the query.

### 4. Live indexing and recovery

Exercise two independent query processes during recording. Test simultaneous
index creation, writer ownership, bounded lock waits, stable read snapshots,
and files arriving after the discovery cutoff. Inject crashes around commit,
sequence gaps, file replacement/deletion, late definitions/completions, clock
invalidation, and incompatible schema versions.

Gate: no duplicate counts, mixed file application, falsely sealed recording,
or silent stale results. Rebuilding from the same evidence yields equivalent
rows. The application continues recording independently of query failure.

### 5. Minimal playground integration

Connect a small execution list and retained-call/value inspection view through
the same `baml_query_btel` API. Review `playground_server.rs`, the old
`playground_telemetry.rs`, and the existing TypeScript telemetry client as
integration references; the old code is not evidence that its routes still run.
Keep blocking SQLite/CAS work off the LSP async executor.

Explicit refresh reconciles then queries. Repeated reads can reuse an open
database; they still receive a new read snapshot and query-scoped CAS cache.
Reuse existing UI pieces where possible, but represent unsupported timeline,
media, or derived metrics honestly instead of inventing legacy rows.

Gate: a real local recording appears, newly published calls become visible on
refresh, and a captured value can be inspected through the shared query path.
Full legacy playground parity is a later catalog/integration task.

### 6. Measure, review, then expand

Benchmark the CLI and shared library in release builds on actual disk. Include
small and growing recordings, many segment files, spawning, repeated captures,
and unique captures. Use fresh processes for CLI measurements and separate
same-process playground measurements. Report OS-cache conditions explicitly.

Measure separately:

- First-query discovery, import, query, and total latency with no database.
- Unchanged-source latency across new CLI processes and a reused connection.
- Incremental refresh cost for a fixed small addition to increasing history.
- CAS-filter latency and decode counts for repeated versus distinct values.
- SQLite size, WAL/checkpoint behavior, peak memory, and bytes read/written.
- BAML execution throughput, CPU, and drain with no reader versus a reader
  refreshing once per second. Also compare before/after the cold metadata change.

Compare repeated queries with a forced-rebuild baseline over the same data and
semantics. Do not compare against unrelated old recording formats as though
they did equivalent work. Retain raw measurements and report repeat ranges.
An alternative persisted DataFusion cache remains possible, but implementing
another backend is not part of this prototype.

Hard gates are correctness and work avoidance: zero unchanged BTEL decodes,
zero metadata-only CAS reads, exactly-once application, bounded memory/work,
and process-safe live reads. Establish latency targets on the fixture before
optimizing; report query and concurrent-recorder costs separately. If fresh
queries reread/decode unchanged history or monitoring materially slows recording, resolve
that before expanding tables.

Run focused reader/index/SQL/CAS tests, CLI and playground integration checks,
affected recording regressions, formatting, and affected-crate Clippy. Run a
broader workspace build when switching consumers or removing dependencies.
After the prototype passes, decide the full catalog and remove the obsolete
query path. Keep the completed recorder experiment available as evidence.

## Deferred scope

Multi-source federation, user-cache placement for read-only sources, background
watchers/daemon ownership, full legacy SQL/catalog compatibility, complete
timeline/media UI, recursive CAS dedup changes, S3 delivery, and the cloud SQL
service remain follow-up work. The prototype supports a writable source on a
local filesystem; unsupported source/storage configurations get explicit errors.
These limits are prototype scope, not a redefinition of product requirements.

## Design references

- Vaibhav's local architecture draft:
  `/media/tony/WesternDigitalNvmeSsd/Code/query_plan.flushded_out.md`.
- Recording comparison and raw evidence:
  `/home/tony/Desktop/Code/baml-worktree-archives/btel-sqlite-benchmark-2026-09-23/files/baml_language/tools/telemetry_storage_results/README.md`.
  The archive also preserves the implementation, a binary-capable tracked diff,
  base commit, restoration instructions, and a verified file manifest.
- `TASK/baml-query-*` documents describe the retired system. Use them for
  historical behavior/examples, not as the implementation mandate for this work.

## Implementation record

Written 2026-09-23 by the implementing agent. The plan above is unchanged
except for its status line; this section records what actually happened.

### What exists now

| Piece | Where | What it does |
| --- | --- | --- |
| Recorded function metadata | `btel_types`, `bex_vm_types`, `bex_heap`, `btel_publisher`, `bex_engine` | Publishes each referenced function's name and argument layout once per recording. |
| CAS decoder | `btel_snapshot/src/decode.rs` | Reads a blob into an owned graph and rejects it unless the recomputed ID matches. |
| One-file validation | `btel_file::validate_file` | The checks `read_directory` used, callable on one file. `read_directory` now uses it. |
| `btel_reader` crate | `crates/btel_reader` | Discovery, one-file reads, clock rules, CAS access, value navigation, comparison and rendering. No SQL. |
| `baml_query_btel` crate | `crates/baml_query_btel` | SQLite index, ledger, refresh, catalog views, SQL translation, value functions, output formats. |
| `baml query` | `baml_cli/src/query_command.rs` | Refreshes, then queries. Table, JSON and JSON-lines output. |
| Playground | `baml_lsp_server/src/playground_btel.rs` | Playground engines now record. The telemetry tab's list and open messages are served from the index. |

### Where this differs from the plan

**Function names were not recorded at all.** On this base the publisher wrote
only `MetadataUnavailable` placeholders, so every recording lacked function
names, not just argument names. The producer change is therefore bigger than
"add a field". At engine start, the engine copies the metadata of every
compile-time function into an owned table (`BexHeap::static_function_metadata`).
The publisher sends a function's metadata the first time a call path refers to
it, once per recording. Functions created at runtime are not in the table and
keep the old placeholder. The argument layout is the optional field the plan
describes (`FunctionMetadata.argument_layout`, format minor 1). It lists
`param_names` when they name every captured slot; a method's first slot is
`self`. Otherwise the field is absent. Nothing in VM frames, function objects,
transport records, captures, GC or CAS encoding changed.

**Type descriptions inside blobs needed their own bound.** Blobs store types
with Borsh, and Borsh decoding recurses once per nesting level. Measured stack
use was about 8 KiB per level in debug builds and 1 KiB in release, so a byte
limit alone was not safe on a 2 MiB thread. Descriptions up to 128 bytes decode
in place. Larger ones, up to 4 KiB, are only measured, on a short-lived thread
with a 64 MiB stack, and kept as verified bytes. A blob with a bigger type is
reported as unavailable.

**The old `baml_query` crate is untouched but no longer used by the CLI.**
`baml_cli` declared it without using it; that dependency now points at
`baml_query_btel`. Removing the old crate is the later catalog decision.

**Build setup.** Builds used this worktree's own `target/`, not the main
checkout's. My notes and `~/.cargo/config.toml` record that sharing a target
directory between baml worktrees reused stale artifacts, and the main checkout
was on a different commit. Incremental compilation is off because its cache
filled the disk once (16 GB). Both target directories were cleaned mid-task to
free space.

**The first measurement runs found three slow query plans, now fixed.** The
index keeps no SQLite statistics, so SQLite guesses how to run each statement.
Three times it guessed "read every row of this recording" inside a loop that
already runs once per row:

- The `executions` view found each execution's entry function by reading all
  of the recording's call paths. On a recording with 13,888 executions, that
  one column took 9.3 s. A covering index now lets it read only that thread's
  call paths: 16 ms.
- Its `retained_calls` column read every retained call in the recording for
  each execution. On 13,568 executions that took 56 s. A `CROSS JOIN` now makes
  SQLite start from the execution's own threads: 12 ms.
- After each transaction, refresh looked for unreported call conflicts and for
  threads still missing their execution by reading the whole recording. So each
  one-second refresh of a live recording got slower as the recording grew. Two
  partial indexes now hold only those pending rows, and the statements name them
  with `INDEXED BY`. If a later change makes one unusable, the statement fails to
  prepare and every refresh test fails.

A test in `catalog.rs` asks SQLite for the plan of `SELECT *` on every relation.
It fails if a lookup inside a loop narrows only by recording. Undoing either
view fix makes it fail. The schema version is now 2, so an older index file is
rebuilt on first use. The timings above are for each subquery alone. A full
`SELECT * FROM executions` on the 13,888-execution recording went from 3.1 s to
33 ms.

### Decisions made along the way

- Identities (thread, call, function, epoch IDs) are stored as 8-byte
  big-endian blobs, so they are never read as signed numbers. Ticks, counts and
  tick totals are stored as integers only when they fit; otherwise the column is
  NULL and an issue or `overflow` state says why. Sums use a checked aggregate.
- Public IDs are text: `recording_id` is 32 hex digits; execution, call and
  parent IDs are `<recording_id>:<number>`. They come from the evidence, so a
  rebuild gives the same IDs.
- Relations are TEMP views created on each connection, so a view always
  matches the binary running it. Internal tables carry the data.
- A refresh with nothing new writes nothing. That covers an unchanged invalid
  file too: it is remembered and not decoded again.
- Switching a new database file to WAL happens under the writer lock. Without
  it, four processes opening a fresh project at once got "database is locked".
- `baml query` exit codes follow the ones already defined for it: 0 complete,
  1 incomplete evidence, 2 invalid SQL, 3 budget, 5 other failure.

### How queries treat captured values

`args`, `output` and `error` columns hold a small handle: which blob, which
root, which path. Nothing is read to build one. Navigation such as
`args['customer']['age']` adds path steps. A blob is read only when a value is
compared, rendered or tested, and each blob is read at most once per query.

- A captured `null` is data. An absent key, an out-of-range index or an
  omitted argument is `NULL` and does not make the answer incomplete.
- A missing or corrupt blob, a value cut by capture limits, or unknown
  argument names also give `NULL`, but the outcome becomes `incomplete` and
  names the reason. For example, `cas_missing` or `argument_names_unknown`.
- Comparisons do not use SQLite's coercions. `output = 27` matches the int
  27; `output = '27'` does not. Ints, floats and bigints compare exactly. An
  enum equals the text of its variant name.
- Handles pass through CTEs and FROM subqueries unchanged, so an outer query
  can keep navigating. Names are bound per scope; a CTE column called `output`
  is plain text, not a value.
- Whole values render as JSON with `$`-markers for what JSON lacks:
  `$class`, `$enum`/`$variant`, `$bigint`, `$omitted`, `$truncated`, and
  `$id`/`$ref` for shared or cyclic objects. JSON output restores booleans and
  structured values; the table shows text.

### Catalog as built

Stages 1 to 5 built `recordings`, `executions`, `calls`, `function_stats` and
`issues`, as planned, plus two columns for the playground:
`executions.started_at_ms` and `calls.start_offset_ns`. The reader phase
extended these and added eleven relations; see
[Reader phase](#reader-phase-2026-09-24). Run `baml query --schema` for every
column and its meaning.

### Tests

| Suite | Count | Covers |
| --- | --- | --- |
| `btel_snapshot` decode | 7 | Round trips of every value kind, ID verification, every truncated prefix and every single-byte corruption rejected, limits before allocation, deep types never recursing on the caller. |
| `btel_reader` | 8 | Navigation outcomes, comparison semantics, rendering with cycles and sharing, clock rules, UTC formatting. |
| `baml_query_btel` unit | 9 | SQL translation: brackets, BAML comparisons, CTE and subquery flow, a name reused across scopes, wildcards, rejected SQL. Query plans: no relation rescans a recording per row. |
| `baml_query_btel` engine | 5 | Real engine recordings: the acceptance queries, zero CAS reads for metadata queries, one read per blob per query, zero decodes on reopen, live exactly-once application, four concurrent first queries. |
| `baml_query_btel` values | 2 | Omitted arguments, a real cyclic value, missing and corrupt blobs. |
| `baml_query_btel` recovery | 11 | Late definitions and completions, clock invalidation, conflicts, gaps, invalid, changed and deleted files, rebuild equivalence, schema mismatch, lock timeout, fixed read snapshots, processes killed before and after commit, time budget. |
| `baml_cli` query | 2 | Real `baml run` then `baml query` processes: the first query builds the index, later ones decode no files, a new run adds exactly its files; schema, invalid SQL, stdin. |
| `bex_engine`, `bex_vm`, `btel_publisher` | 3 new | Metadata published once, layouts aligned with captured slots, absent layouts. |
| `baml_lsp_server` | 1 | A playground engine records; runs appear on refresh; captured values render. |

Existing recording tests in `bex_heap`, `btel_*` and `bex_engine` still pass.

### Measurements

Measured 2026-09-23 on a Ryzen 9 5950X with ext4 on NVMe, release builds. Other
agent sessions were running, and the load average was 6 at the start, so some
runs have outliers. Every cell below is a median of 5 to 10 fresh processes.
The raw rows are retained locally in
`baml_language/tools/btel_query_results/raw.jsonl`; generated benchmark data is
not included in the PR. To
print every table, run `python3 tools/btel_query_summary.py
tools/btel_query_results` from `baml_language`. `tools/btel_query_bench.py`
reruns everything; the recorder half builds the same benchmark file against
the base commit, so before and after run the same work.

The workloads: `tiny` runs one call per execution. `dense` makes 1,000 calls
per execution, and `spawn` spawns 32 threads per execution. `capture-repeat`
captures the same 64 KiB argument every time. `capture-unique` captures a
different 64 KiB argument every time, so every call writes a new blob.

**The hard gates pass.**

- A query never decodes a file it has already indexed. A refresh that finds
  nothing new takes 0.2 to 1.1 ms, or 2.4 ms with 613 files.
- Queries that only touch metadata read 0 CAS blobs.
- A blob is read at most once per query. Rendering 200 values from 2 distinct
  blobs reads 2 blobs.
- The metadata change does not slow recording. Recording speed on this branch
  matches the base commit within run-to-run noise on every workload. For
  example, `tiny` records 57.1k executions/s here and 55.3k on the base.
- A `baml query` once per second next to a recording process costs little.
  `tiny` drops 1.6%; `dense` and both capture workloads do not move. `spawn`
  drops 6% at the median (4.6k vs 4.9k executions/s), but the two ranges
  overlap almost completely (4.0k to 5.0k vs 4.3k to 5.0k).

**Where the time goes.**

1. Indexing reads about 16 MB of recording per second. The first query on a
   51 MB recording takes 2.9 s, and on a 226 MB recording it takes 13 s. It
   writes 2.5 times the recording's size to disk, and the finished index is
   1.1 to 1.5 times the recording's size. `tiny` records about as fast as
   this, so a reader running during `tiny` only just keeps up.
2. The views cost about 2 µs per row, every query. `SELECT count(*) FROM calls`
   takes 1.05 s on 514,560 calls, even when the refresh before it took 24 ms.
   `function_stats` grouped by function takes 2.8 s on the `spawn` recording.
   In the growth test, refresh time stayed at 21 to 24 ms while history grew
   from 12 to 212 files. But the whole query grew from 72 ms to 1.1 s, all of
   it that count.
3. Filtering on a small value reads the whole blob.
   `WHERE args['n'] = 7` over 20,408 unique captures reads 1.43 GB of blobs.
   It takes 0.59 s with the blobs in the page cache and 4.9 s without.
4. Rebuilding is what incremental indexing avoids. With 212 files (196 MB)
   indexed, adding one more file takes 24 ms. Deleting the index and
   rebuilding it takes 11.9 s.

**Memory.** A query that finds nothing new peaks at 79 MB, which is the CLI's
own baseline. A first query peaks at up to 283 MB. The unique-capture filter
peaks at 286 MB.

**The playground case.** One `Index` reused across queries, as the playground
does, refreshes in 0.1 to 1.6 ms when nothing is new. After that, each query
costs only its SQL. Blob reads are not cached across queries, so the
unique-capture filter reads its 20,408 blobs every time.

**Things that look wrong but are not the reader.**

- `spawn` runs 16% faster with recording on than with it off. This is on
  both the base commit and this branch, so it predates this work. Nobody has
  looked at why yet.
- The raw data says the index uses `journal_mode = delete`. It does use WAL.
  The harness opens the file with `immutable=1`, and SQLite then always
  reports `delete`.

**Decisions for next.** Resolved on 2026-09-24: Tony chose functional
coverage first, without latency targets; see
[BTEL query expansion](btel-query-next-phase.md). The questions as they were
asked:

- Is 16 MB/s acceptable for building the first index? If not, ingest is the
  place to work.
- Should the public views become tables filled at ingest? That would remove
  the 2 µs-per-row cost from every query.
- Should small captured values be stored in the index, so filters like
  `args['n'] = 7` skip the blob?
- Then the full catalog, and removing the old `baml_query` crate.

### Reader phase (2026-09-24)

Done under [BTEL query expansion](btel-query-next-phase.md): functional
coverage first, producer frozen. `python3 documents/check-producer-baseline.py`
reports that none of the 153 frozen producer files differ. The
[demo checkpoint](btel-query-demo.md) runs every acceptance question with
`baml run` and `baml query` and shows real output.

**Relations now.** Old names kept, with their earlier columns:

| Relation | One row per | New in this phase |
| --- | --- | --- |
| `recordings` | recording | `seal_state`, `prefix_state`; `state` says `unsealed` instead of `live` |
| `executions` | known root thread | entry id and state, end time, Unix ns, `completed_calls`, retained counts by outcome, `threads_total`, completion and aggregate states |
| `threads` | thread, including referenced-only ones | new: parent node and kind, parent thread, spawning call, spawn context and function, root or spawn, offsets, lifetime, end status |
| `calls` | retained call | parent kind and parent call, call path, reentry, function id and kind, structure and evidence states, raw ticks, end times, separate args, output and error states and content ids |
| `error_calls` | retained errored call | new; the `calls` columns |
| `call_paths` | calling context | new: parent, depth, caller function and bytecode pc, callee, edge |
| `call_path_nodes` | context and reentry bit | new: aggregate totals as recorded, with exact decimals |
| `call_path_stats` | calling context | new: normal and reentry counts, inclusive, reentry, invocation-sum, direct-child, await and self time, with states |
| `hot_call_paths` | context with a supported self time | new |
| `function_stats` | execution and function | now summed from `call_path_stats`; adds `completed_calls`, `invocation_duration_sum_ns`, `await_ns`, `self_ns`; old names kept as aliases |
| `function_definitions`, `function_parameters` | referenced function; recorded slot | new |
| `clocks` | clock epoch | new: every recorded conversion, calibration and UTC field, observed status |
| `issues` | problem | adds `severity`, `subject_kind`, scoped subjects and derived problems: undefined threads, paths, functions and clocks, unresolved parents, invalidated clocks |
| `recording_files` | applied or rejected file | new |
| `capabilities` | old-tracer capability | new: supported, partial or unsupported, with where to look |

`errors`, `health`, `processes`, `store_files`, `value_index` and
`cct_population` are refused with an explanation of what replaced them.
`baml query --schema` ends with the value functions, the capability list
and those retired names.

**Decisions.**

- IDs. Executions, threads and calls share one namespace,
  `<recording_id>:<n>`, because a wire parent can be either. An execution's
  id is its root thread's id. Paths, functions and clocks are
  `<recording_id>:p<n>`, `:f<n>` and `:c<n>`. `calls.thread_id` and
  `executions.thread_id` changed from a bare number to this form so joins
  across recordings are safe.
- Placeholders. A reference to a thread, path, function or clock with no
  indexed definition gets a placeholder row. It stays visible as
  `unresolved` and in `issues`. Only a defined thread without a parent is a
  root, so a missing parent never invents an execution. A later definition
  fills the placeholder in place.
- Depth. A path's depth is stored when all its ancestors are defined and
  repaired when a late parent arrives, like execution roots.
- Recursion. A direct recursive call reuses its caller's path with the
  reentry bit. `inclusive_ns` is the outermost invocations' time.
  `invocation_duration_sum_ns` adds the re-entries, so it counts nested
  time again. Self time is inclusive minus the inclusive time of direct
  synchronous child contexts minus the await time of outer and recursive
  frames. Spawned contexts are not child time.
- Self time is never clamped. `self_time_state` is `valid` on a finished
  thread, `provisional` on an unfinished one, `incomplete` when the outer
  invocation has no completion yet (or children exceed it on an unfinished
  thread), `underflow` when they exceed it on a finished thread, or the
  clock's state.
- Counts are completed invocations from aggregates. Retained calls are never
  added to them, because the processor already counts every retained
  completion in its aggregates.
- Aggregate totals keep an exact decimal copy. The INTEGER column is NULL
  once a total passes i64, and the state says `overflow`.
- A clock epoch with conflicting definitions makes timings `conflicted`,
  not `unknown_clock`.
- Value SQL. A set operation keeps value handles in its branches and
  renders once at the output, so JSON output stays typed. Positional
  `ORDER BY` and `GROUP BY` skip the hidden kind columns.
  `baml_value_state(v)` reports `present`, `null`, `missing`, `omitted`,
  `no_value` or why the value is unavailable; `baml_kind(v)` reports its
  kind. Comparing structured values reports `comparison_unsupported` in
  the outcome instead of an unexplained NULL. Inputs expected from an
  announcement that is not indexed yet report `capture_pending`.
- Vocabulary changes. The outcome's `live` flag is now `unsealed`.
  `calls.args_state` uses `reference`, `pending`, `not_recorded` and
  `conflicted` (was `captured`, `pending`, `none`); `output_state` and
  `error_state` are new. Call and execution status can be `conflicted`.
- Playground. Opening an execution now sends threads, calling contexts and
  calls with their paths. Calling contexts carry a new optional
  `completedCalls` field; `callsStarted` and the population outcome counts
  stay null. An execution without a root completion has a null status
  instead of `running`. Error entries keep the value but leave the throw
  call, function and site null, since the recording does not say where the
  error was thrown. The TypeScript change is one optional field and one
  fallback; it was not type-checked here because this worktree has no
  `node_modules`.

**Where this differs from the v2 draft** in
[the logical schema](btel-logical-schema.md):

- One source: no `sources` relation or `source_id`, and no `_v2` names.
- Status stays `ok`, `errored`, `cancelled`, `incomplete`, now plus
  `conflicted`, rather than `succeeded` and `failed`.
- `timing_state = valid` means no invalidation was observed so far; there is
  no separate `provisional`, because no producer ever marks an epoch final.
- Not built: `recordings.observed_started_at`, `executions.active_issue_count`,
  `issues.active` and `issue_id`, per-file `aggregate_deltas`,
  `value_index` and `value_locations`.
- Unsupported comparisons are reported in the outcome rather than rejected.

**Tests added.** `structure` (6, hand-built files with exact values):
recursion-aware statistics, every self-time state, clock invalidation
keeping counts, unresolved structure resolving when late evidence arrives,
exact totals past i64, conflicting thread completions. `hierarchy` (2, real
engine recordings): a thread spawned inside a retained call, nested
retained calls, `Fib(6)`'s 25 invocations on one context, one callee at two
call sites, population totals agreeing across three relations, typed
`UNION` output, value states, structured-comparison diagnostics, caught and
uncaught errors. Four translator tests; a catalog test that every
documented column matches its view. The recovery, engine and playground
tests were extended where the vocabulary or payload changed.

**Verified** on the pinned 1.98.0 toolchain: `btel_reader`,
`baml_query_btel`, `btel_snapshot`, `btel_file`, `baml_cli` (`query_e2e`,
`profiling_unavailable`) and `baml_lsp_server` tests pass; `cargo fmt
--check` and Clippy with `-D warnings` on all targets are clean for the four
changed crates. Not run at the time: the TypeScript tests (run later, see
[Playground follow-up](#playground-follow-up-2026-09-24)), a full workspace
build, and any benchmark.

### Playground follow-up (2026-09-24)

A review of the TypeScript consumers found that the views still turned
missing numbers into facts:

- The adapter turned `completedError: null` into 0 errors, and null self
  time, inclusive time and counts into 0. Views then showed "0 errors" and
  summed partial totals.
- The overview presented every errored retained call as a distinct throw
  (a "1 error" headline, a stack section), and could say "No call errored"
  when outcome counts were not recorded at all.
- A null execution or call status was shown as `running`.
- `run_demo.sh` ignored every CLI failure.

**What changed.**

- Protocol. Error entries carry `grain`: `throw` (the default for older
  backends) or `errored_call`, plus the call, function and thread the value
  was seen on. Executions and calls can be `incomplete`: no end is recorded,
  with no claim that anything is still running. The LSP now sends both.
- Adapter (`evidence.ts`). Counts, errors, self and inclusive time, and
  subtree waits stay null when unknown. A total over any unknown part is
  null. Gaps are only subtracted from known counts. A duration summary is
  never called the population when the count is unknown. `incomplete` is its
  own status and is not treated as lost records.
- Views (`TelemetryView.tsx`). Unknown values show as "not recorded" or
  "—", sort last, and never feed a percentage, bar or rate. Errored-call
  entries get their own panel: one row per call, never merged by value,
  saying where they were thrown is not recorded. The throw panel is only for
  `throw` entries. With no recorded outcome counts, the overview counts
  retained errored calls instead of claiming how many calls errored. An
  incomplete run shows its evidence with a note that it may still grow, and
  is polled like a running one. The flame draws measured time only;
  contexts with unknown time have no width there but stay in the table.
- Demo script. Every CLI call must exit with the expected code. The mock
  server gets a free port and a readiness check. The background run is
  waited for. Everything is deleted only inside a directory the script
  creates. A fake CLI that always exits 3 stops it at the first command
  with `expected exit 0, got 3`.

**Checks run.**

- TypeScript, in `typescript2/pkg-playground`: `tsc --noEmit` is clean;
  `vitest run` passes 29 files and 240 tests, including 13 new ones in
  `src/telemetry/unknown-evidence.test.tsx`. Those cover unknown versus zero
  counts, partial totals, unknown waits, sample versus population, errored
  calls versus throws, equal values staying separate rows, the old throw
  panel still rendering for throw captures, and `incomplete` versus
  `running`. Two components are rendered to static markup (the overview and
  the context inspector) and their text is asserted. `biome check` is clean
  on the seven changed files.
- Setup for those checks: dependencies installed with pnpm 11.1.3 from the
  local store, filtered to this package; one missing package (`yaml`) was
  fetched with `--prefer-offline`. The gitignored protobuf TypeScript in
  `pkg-proto` was generated with its local `buf` plugin. Node 22 was used;
  one unrelated app package asks for Node 24.
- Rust: the playground test now also records a run whose retained call
  throws, and checks the `errored_call` entry and its null throw fields.
  The `baml_lsp_server` tests pass, and Clippy with `-D warnings` and
  `cargo fmt --check` are clean.
- `run_demo.sh` ran end to end against the debug CLI built at 14:50 today
  and exited 0; its transcript is `documents/btel-query-demo/output.txt`.
- The producer baseline check still reports none of the 153 files differ.

**Not verified.** Nobody has driven the playground in VS Code or a browser
since these changes; the UI claims above rest on the unit and static-render
tests. The behavior for older backends is covered only by the existing
fixture tests, since no older backend serves this playground on this
branch.

### Cloud-stack integration (2026-09-24)

The query PR is now based on #4958, the ninth producer-stack PR. The existing
query/schema/UI work is preserved. Integration changes are limited to:

- Import `btel_recorder` after the upstream crate rename, and use its shared
  `RecordingBuilder`. Pass the prototype's existing cold function metadata
  through both local and cloud publisher initialization. No query/index work
  is added to recording.
- Use the upstream typed hash-domain constants in the decoder. VM dispatch
  and production telemetry, the processor, CAS encoding and CAS hashing match
  #4958; the VM telemetry diff contains only the existing metadata unit test.
- Use the upstream `BAML_TELEMETRY=medium` spelling in the CLI test and
  measurement helper. It replaces `auto`; the old measurement results keep
  their original configuration and base. Any new recorder comparison needs a
  baseline binary built on #4958.
- Update cloud upload fixtures for the prototype's existing recording minor
  version 2.1. Their recording headers gained the two-byte minor field, so
  recording/envelope lengths and SHA-256s changed. CAS bytes/IDs and server
  plans are unchanged; `protoc` independently re-encoded the outer envelopes.
- Keep the original 153-file phase-A manifest as historical evidence. The
  checker now defaults to the separate 164-file post-integration manifest,
  `btel-query-cloud-base-producer-baseline.json`. It includes upstream source
  changes and the adapted cold metadata setup; it does not imply the producer
  stayed byte-identical across different upstream bases.

Validation after integration, on pinned Rust 1.98.0:

- All query tests pass: unit/SQL, real-engine values/hierarchy, recovery,
  synthetic structure/timing and value projection.
- Snapshot, reader, recorder, local-file and cloud suites pass, including the
  fixed upload-wire fixtures and decoding the frozen CAS v1 fixtures.
- Real-engine local recording, function metadata and cloud delivery tests pass.
  The cloud test now checks that named function definitions and their argument
  layouts reach the uploaded recording.
- Both CLI query tests, profiling-unavailable coverage and all 76 LSP tests
  pass. The CLI demo was rerun with the rebased debug binary; the transcript
  and all ten documentation output blocks were regenerated from that run.
- Clippy with warnings denied passes on the 12 affected crates and their
  targets. The TypeScript files are unchanged by this integration; their
  preceding 240-test/typecheck result still describes those same sources.

No performance measurements or full-workspace build are part of this integration.

### Known limits

- A normal shutdown now writes `RecordingEnd` and final clock states once
  every recorded run settled; see
  [recording completion](btel-query-recording-completion.md). Recordings from
  older producers, crashes before the end marker is written, and shutdowns
  that abandon an observed spawned thread stay `unsealed`. An execution
  without a completion shows as incomplete; nothing says whether it is
  still running.
- `timing_state = valid` with `is_final = 0` means no invalidation was seen in
  the indexed files. With `is_final = 1` the status can no longer change.
- Functions created at runtime have no names.
- An execution's entry function is known only when its root thread recorded
  exactly one top-level call path.
- Structured equality works for supported, fully captured acyclic values.
  Cycles, opaque/type/descriptive values, capture truncation and comparison
  limits remain explicit; structured ordering is unsupported. See the
  [comparison contract](btel-query-value-comparisons.md).
- `UNION` (without `ALL`) compares value handles, so two equal values from
  different blobs or paths are not merged.
- Skipping unchanged files trusts size, mtime, device and inode.
  `baml query --verify` hashes every indexed file instead.
- Blob reads happen inside SQLite callbacks on the querying thread.
- The playground has no throw stacks, call-site lines, thread names or media
  bytes; they are sent empty or null.
- The playground polls an `incomplete` execution every 2 seconds while the
  Telemetry tab is open, including one whose writer died and will never
  finish; each poll is a refresh that finds nothing new.
- Ticks beyond the signed 64-bit range are NULL in the index with a
  `tick_out_of_range` issue; the recording file keeps the raw value.
- Views still compute many derived columns per row. `executions` runs about
  ten correlated lookups per row. Synchronous-child tick sums are now cached
  at indexing time, removing their repeated calculation in `call_path_stats`.
  See [short reader performance checks](btel-query-performance.md) for the
  repeatable measurement command and the scope of that optimization.
- Cloud delivery and Boundary Studio are not part of this work.

### Recording-completion follow-up

Normal shutdown now publishes recording completion and final clock evidence
using existing epoch lifecycle state, without changing VM hot-loop code or
layouts. The [completion record](btel-query-recording-completion.md) covers
the lifecycle contract, 222 passing tests and remaining failure distinctions.
The [upstream performance comparison](btel-query-upstream-performance.md)
took 108 seconds of measurements against the untouched profiler base. The
[remaining-question audit](btel-query-remaining-questions.md) lists the old
queries still unsupported or only partially answered, separately from SQL
spelling changes and new feature requests.
