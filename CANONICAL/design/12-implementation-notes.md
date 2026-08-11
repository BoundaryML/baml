# Implementation notes

**Status:** Working decision ledger for the C0–U1 implementation milestones. Each entry records a choice made during implementation where the canonical set deferred, was silent, or required a code-verified fact. Semantic changes still follow the [change-control rule](09-delivery-plan.md#change-control-rule); entries here are either freeze-gate resolutions or implementation-only choices, and say which.

## How to read this

- **IN-C\*** entries close C0/C1 items; **IN-Q\*** entries close Q1–Q3 freeze gates; **IN-U\*** entries close U1 items.
- “Evidence” cites the code fact that forced or justified the choice.
- A decision marked **freeze** becomes part of the public v1 contract; one marked **impl** may change behind the contract without notice.

## C0 — canonical contract verification

### IN-C0-1 — Gate verification results (2026-08-11)

The C0 gate was verified mechanically:

- every archived source under **CANONICAL/archive** has a disposition row in the [source map](11-source-map.md);
- every relative link and `#anchor` in **CANONICAL/\*\*.md** resolves;
- every occurrence of a superseded term (**meta.bamlmeta**, **.bpk1**, **index.jsonl**, **StudioQueryV1**, **chDB**) outside the archive is an explicit correction/supersession entry, never a current claim; and
- every design document opens with a **Status** line separating built from target work.

The validator lives at [tools/validate_docs.py](../tools/validate_docs.py) and must stay green when canonical documents change.

## C1 — profiler hardening decisions

### IN-C1-1 — Root-pin durability protocol (**impl**, invariant-affirming)

One crash-safe barrier: value-segment fsync → pack fsync (data + one-time
packs-dir entry) → manifest append fsync (file + boundary dir). Renames
(pack seal idx, GC compaction, dictionary publish, flight dumps) are
durable tmp-fsync → rename → dir-fsync. Dedupe trusts only provably
durable chunks: sealed idx ⇒ durable (seal fsyncs first); a crashed
writer's pack is recovered durably at open (single-shot, dead-lease
cleanup); a live writer's unsealed pack is readable but never absorbs
another writer's put (same-process ownership resolved exactly via an
active-writer registry, foreign pids via a conservative `kill(0)` probe).

### IN-C1-2 — Structural-exhaustion policy (**freeze** of mechanism; default remains X1-overridable)

`BAML_PROFILE_EXHAUSTION` = `fail_run` (default, per the decision
register's recommendation) | `abort_process` (strict opt-in, the historic
abort) | `continue_incomplete` (diagnostic admission: shed under
pressure, resume when drained). All modes count drops per ring; the
consumer persists SHED markers, degrades partitions, and boundary
completion carries the delta plus a `BoundaryLoss` record. `fail_run`
does not kill or fail the user's process locally — it fails the *run's
structural evidence* explicitly; hosted delivery-required semantics
remain H-milestone work.

### IN-C1-3 — Counter overflow: persist saturation, don't widen the wire (**impl**)

BCCT v1 wire stays u32 for count deltas. In-memory histograms saturate
(drops counted); every engaged wire clamp is counted; a fold that
clamped embeds a SATURATED marker in the sealed snapshot. Rationale:
overflow needs >4.29 G calls per node per boundary locally — explicit
lower-bound evidence is honest and format-stable; widening can ride a
future BCCT major version if hosted scale ever demands it.

### IN-C1-4 — Full trace and helper staging (**decision recorded**)

Full-trace writer: **not required for v1**, stays explicitly absent
(already canon-deferred). Speculative helper staging/promotion: machinery
and tests stay, production wiring stays deferred (canon-deferred); the
root-error `promote_staged` call remains a structural no-op and no user
surface may imply retroactive helper capture ships.

### IN-C1-5 — Continuous CLI value drain (**impl**)

A per-boundary `baml-value-drain` thread drains captured drafts every
250 ms (encode + segment append + CAS put off the application threads)
and runs the IN-C1-1 barrier at stop; inline drain remains the
spawn-failure fallback. The value segment is created lazily at the first
draft (no more header-only files). wasm/playground hosts still drain
inline — host parity is tracked, not pretended.

### IN-C1-6 — Memory bounds (**impl**)

`free_partition` recycles dead thread slots (free list, tombstoned
`thread_id`); the defer list is bounded by `DEFER_MAX_PENDING` (64 Ki) —
at the cap the engine resolves everything resolvable through the
existing declared-synthesis path immediately. Residuals per completed
boundary: one small `Partition` stub; node rows remain epoch-bounded.
`thread_slab_occupancy`/`pending_deferrals` expose the bounds.

### IN-C1-7 — Hot-path budget re-measured

cct_engine bench after C1: hotloop 48.6 ns/pair (baseline 47.9; gate
≤50), p99 shape 54.4 (baseline 54.1; pre-existing >50, within the 60
never-exceed), migration unchanged. The +0.7 ns is the histogram
saturation branch on the call-end path.

### IN-C1-8 — Disk-full qualification (2026-08-11)

Manual qualification on a 256 KiB tmpfs mounted as `.baml`:

- **Filesystem full before begin:** boundary begin fails with an explicit
  verbose diagnostic; the run proceeds unobserved and exits 0 with
  correct program output. No panic, no partial garbage.
- **Filesystem fills mid-boundary:** the dictionary write fails with a
  declared fallback diagnostic; boundary meta, sealed `cct.bamlcct`,
  values, manifest, and sealed pack+idx still land; the run exits 0.
  Reads honor the degraded contract — missing dictionary yields honest
  `fn#<id>` numeric labels, exact counts, `sealed=true`.

A portable automated ENOSPC suite needs a fault-injection seam below the
store (or CI privileges for loop/tmpfs mounts); tracked as follow-on
hardening, with this qualification as the recorded evidence.

## Q1 — catalog and query-core freeze resolutions

### IN-Q1-1 — `args` root shape: named-argument object (**freeze**)

`args` is an **ordered-by-name argument object keyed by declared parameter name**, not a positional list.

- `args['customer']` selects the argument named `customer`.
- A **numeric subscript directly on the `args` root is a planning error** with a remedy (“subscript `args` with the parameter name, e.g. `args['customer']`”).
- Numeric subscripts remain available for genuine list values: `args['items'][0]`.

Evidence: the VM captures call inputs as named entries — `maybe_capture_call_inputs` pairs each stack value with its declared parameter name (`argN` fallback) and the trace heap stores them as `TraceValue::Map`
(`bex_vm/src/vm.rs`, `bex_engine/src/trace_heap.rs::copy_named_values_from_bex_heap`). The canonical codec then **sorts map keys by UTF-8 bytes and does not preserve declaration order** (`bex_events/src/store/canon.rs`). A positional root would therefore index sorted-key order, not signature order — a silent correctness trap. Named access matches both the stored artifact and what users see in their own signatures.

The illustrative `args[0]['customer']` spellings in [the query examples](../PROJECT_STUDIO_QUERY_EXAMPLES.md) predate this freeze; that document explicitly deferred the root shape to Q1. Its examples are updated by this decision.

### IN-Q1-2 — Subscript grammar and index base (**freeze**)

- `value['key']` — string subscript selects a class field or map key.
- `value[N]` — integer subscript selects a list element, **zero-based** (BAML semantics; the deviation from SQL's one-based arrays is deliberate and documented at the grammar surface).
- Subscripts chain arbitrarily; both spellings are ordinary SQL expressions lowered by the analyzer to internal traversal expressions (D7). Internal function names are private.

### IN-Q1-3 — Available value, absent path / type mismatch (**freeze**)

For an **available** value:

- a subscript whose path does not exist evaluates to SQL `NULL` (predicate non-match; the result stays **complete**);
- a comparison against an incompatible leaf kind (e.g. `['age'] >= 30` where `age` is a string) evaluates to SQL `NULL` (non-match; result stays complete);
- a captured BAML `null` is ordinary SQL-`NULL`-like data.

These three are deliberately indistinguishable in v1 predicates (JSON-path-family ergonomics) and all **distinct from unavailable evidence**, which never reaches this evaluation. Statically provable misuse (numeric subscript on the `args` root; subscripting a scalar literal) fails at planning time instead.

### IN-Q1-4 — Typed unavailable carrier (**freeze**)

Unavailability is carried without altering ordinary SQL row schemas:

- resident per-role state columns (`args_state`, `return_state`, `error_state`) say **why** a value is unavailable;
- a row whose value predicate cannot be decided because the value is unavailable is **excluded from the data stream** and counted in `query_outcome.valueEvaluations.unavailable` / `byReason`, making the result explicitly incomplete (D12/D13);
- a `SELECT`ed unavailable value renders as SQL `NULL` in the data column while the same outcome accounting records the typed reason.

A future strict mode (fail on any unavailable) stays deferred.

### IN-Q1-5 — Run status mapping (**freeze**)

`runs_v1.status` is the closed enum `pending | running | waiting | succeeded | failed | cancelled | panicked | abandoned`.

Mapping from current evidence (writers emit `succeeded`/`failed`/`cancelled` in `BoundaryComplete`; `running`/`crashed` are derived by the reader from session liveness — `bex_query/src/runs.rs`):

| Evidence | Catalog status |
|---|---|
| `BoundaryComplete{succeeded/failed/cancelled}` | same word |
| terminal panic record (when the runtime emits one) | `panicked` |
| begin-without-complete, owning session alive | `running` |
| begin-without-complete, owning session dead | `abandoned` (today's reader word “crashed”) |
| reserved for future host lifecycle stages | `pending`, `waiting` |

`abandoned` rather than `crashed` because the axis names the *evidence* (“no terminal record, owner gone”), not an inferred cause; a panic that was recorded is `panicked`, an abrupt kill and a lost record are both `abandoned`.

### IN-Q1-6 — Whole-value equality and comparison semantics (**freeze**)

- `=` / `<>` over BAML values is **canonical semantic equality**: the equality defined by the canonical codec (`canon.rs`), including canonical NaN (NaN = NaN) and **distinct ±0**.
- Ordering comparisons apply to comparable scalar kinds (int/float cross-compare numerically; strings bytewise UTF-8); a cross-kind ordering comparison is `NULL`-like non-match per IN-Q1-3.
- Provider optimization: because canonical encoding is deterministic, root-CID equality **is** semantic equality for values that were canonically encoded; this satisfies D7's proof obligation. Values retained only as legacy inline/blob bodies must be hydrated and compared semantically — CID shortcuts never apply to them.
- Parameter binding (`:name`): scalars bind from ordinary SQL literals/CLI values; whole structured values bind either as `@cid:bamlv_1_…` (a canonical value reference, the exact-workflow default — users copy the `cid` column from a prior result) or `@json:{…}` with the documented JSON→BAML mapping (object→map, array→list, string/number/bool/null direct). JSON binding cannot express classes/enums/media; class-typed whole-value equality therefore uses `@cid:` or nested scalar predicates.

### IN-Q1-7 — Local catalog v1 contents (**freeze**, local scope)

Local catalog v1 exposes: `runs_v1`, `cct_population_v1`, `retained_calls_v1`, `evidence_issues_v1`, `exact_windows_v1`, `functions_v1`, `revisions_v1`, `llm_population_v1` (provisional), `spawn_edges_v1`.

- `call_sites_v1` and `retained_calls_v1.call_site_id` are **excluded** until the compiler/dictionary call-site producer emits rows ([profiler — call-site identity](03-profiler.md#call-site-identity)); adding them later is an additive, non-breaking catalog change.
- `llm_population_v1` carries `model` identity only — current evidence has no separate provider identity; the `provider` column joins the relation only if Aaron's model keeps provider/model public (per the [query examples §6](../PROJECT_STUDIO_QUERY_EXAMPLES.md#6-llm_population--provisional)).
- `spawn_instances_v1` ships if bounded instance rows decode from the existing Instance blocks; otherwise it waits with `call_sites_v1`. (Resolved during Q2 provider work.)
- Hosted-only relations (`observations_*`) are outside this local milestone set.

### IN-Q1-8 — Crate and dependency layout (**impl**)

- **`baml_query`** — backend-neutral core (catalog, planning, value lowering, budgets, outcomes, `QueryScope`/provider/`ValueResolver`/capability contracts). Depends on DataFusion + `bex_events` (canonical value model/codec — mandated single codec) and nothing host/CLI/cloud shaped.
- **`baml_query_local`** — local providers over `bex_query` readers (fold/runs/values) plus the canonical-CAS `ValueResolver`.
- `baml_cli` gains `baml query` wired to `baml_query_local`.
- DataFusion **54.1.0** verified to resolve and compile on the workspace toolchain (Rust 1.93.0, edition 2024).

The `bex_query` (fold/BQL) crate is unchanged by Q1; renaming/merging remains a non-semantic later choice.

## Build environment

- `RUSTC_WRAPPER=baml-sccache` requires `/root/tmp-build` to exist; a stale sccache server whose temp dir was deleted fails every compile with exit 254. Recreating the directory (or `sccache --stop-server`) fixes it.
