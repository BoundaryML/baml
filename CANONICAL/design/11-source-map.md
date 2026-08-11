# Source map and reconciliation ledger

**Status:** Complete disposition of the [archived source corpus](../archive/README.md) as audited on 2026-08-10. The corpus was moved from the repository root into **CANONICAL/archive**; only relocation/path references were normalized.

## Authority order used

When sources disagreed, this canonical set used:

1. **Live code and tests** for claims about what the current branch already does.
2. [architecture-decisions.md](../archive/architecture-decisions.md) for the current target decisions D1–D16 and explicit open items X1–X4.
3. The newer [profiling-design.md](../archive/profiling-design.md) and [studio-design.md](../archive/studio-design.md) for non-conflicting component detail.
4. [PLAN.md](../archive/PLAN.md) for history, sequencing, and acceptance intent that survived later decisions.
5. Query discussions and old/stale references as rationale, counterargument, or historical implementation context only.

This hierarchy distinguishes “canonical target” from “currently implemented.” A newer decision may define the target while live code still contains the superseded mechanism.

## Disposition of every archived source document

| Source | Role in this canon | Disposition |
|---|---|---|
| [architecture-decisions.md](../archive/architecture-decisions.md) | Current decision authority | D1–D16 copied/reconciled into [Decision register](08-decisions.md); X1–X4 into [Deferred](10-deferred.md). Later decisions override conflicting proposal text. |
| [profiling-design.md](../archive/profiling-design.md) | Newest detailed profiler/local-artifact design | Kept where code verified or where it defines non-conflicting target behavior. Corrected stale filenames, defaults, identity, memory, value-drain, retention, and durability claims in [Profiler](03-profiler.md) and [Local artifacts](storage/local-artifacts.md). |
| [studio-design.md](../archive/studio-design.md) | Newest detailed product/hosted design | Product, capture, storage, security, reliability, and operations material split across the focused documents in this folder. Query-engine choices were superseded where D1–D16 differ. |
| [PLAN.md](../archive/PLAN.md) | Historical synthesis and execution intent | Useful rationale, phase/gate material, code map, and failure concerns retained; obsolete BQL/chDB/query conclusions replaced by current decisions. |
| [stale-profiling-design.md](../archive/stale-profiling-design.md) | Explicitly stale profiler proposal | Historical rationale only. No claim was accepted without confirmation in newer design or code. |
| [stale-studio-design.md](../archive/stale-studio-design.md) | Explicitly stale Studio proposal | Historical rationale only. Earlier storage/query/UI assertions do not override current decisions. |
| [chatgpt-thread-on-subject.md](../archive/thoughts-on-query/chatgpt-thread-on-subject.md) | Exploratory query discussion | Preserved only as rationale/counterargument; not normative. |
| [coworker-not-contextualized-plan.md](../archive/thoughts-on-query/coworker-not-contextualized-plan.md) | Unreconciled alternate plan | Used as an idea inventory, never as authority. Conflicts were resolved through D1–D16 and live code. |
| [review.md](../archive/thoughts-on-query/review.md) | Review of query alternatives | Its useful risks—language divergence, pushdown correctness, budgets, availability, agent ergonomics—are reflected in [Query system](04-query-system.md). Final engine choice follows D3/D6. |
| [IMPLEMENTATION.md](../archive/old-references/IMPLEMENTATION.md) | Older implementation notes | Used to find/code-check historical surfaces. Current behavior is documented from live code instead. |
| [bql-vs-sql.md](../archive/old-references/bql-vs-sql.md) | Older language comparison | Rationale only. BQL is superseded after SQL parity, per [Query system](04-query-system.md). |
| [antibql.md](../archive/old-references/bql-vs-sql-research/antibql.md) | Case against BQL | Incorporated as decision rationale; not a separate contract. |
| [antisql.md](../archive/old-references/bql-vs-sql-research/antisql.md) | Case against SQL | Its semantic-honesty warnings survive as grain naming, typed availability, capability checks, budgets, and outcomes. |
| [as-built.md](../archive/old-references/bql-vs-sql-research/as-built.md) | Snapshot of existing query/UI implementation | Used to inventory current BQL/BQF1/fold surfaces; verified against code before describing them as built. |
| [compose.md](../archive/old-references/bql-vs-sql-research/compose.md) | Proposed synthesis | Useful composition ideas retained where compatible; superseded on public-language/engine choices by D3/D6. |
| [history.md](../archive/old-references/bql-vs-sql-research/history.md) | Evolution/history | Background only; helps explain migration but does not define v1. |
| [steelman.md](../archive/old-references/bql-vs-sql-research/steelman.md) | Strongest BQL case | Its valuable safeguards survive inside the SQL contract rather than as a second language. |
| [studio-contract.md](../archive/old-references/bql-vs-sql-research/studio-contract.md) | Proposed query/product contract | Grain/evidence/product concerns retained; names and engine details reconciled with current decisions. |

## Important corrections made during reconciliation

### Current implementation corrections

| Older or ambiguous claim | Code-verified current state |
|---|---|
| Boundary IDs are ULIDs/chronologically sortable | UUIDv4 payload encoded as **baml_id_1_**; use **created_ms** for chronology. |
| Session metadata is **meta.bamlmeta** | It is **session.bamlmeta**. |
| Packs use **.bpk1** or zstd in v1 | Files are **.bamlpack** with **BPK1** magic; current v1 records are raw. |
| Root history is discovered through **index.jsonl** | No such root index exists; current reader scans boundary metadata under **history**. |
| Values are continuously drained off-thread by CLI | The reusable service exists; current CLI drains once synchronously at boundary finish. |
| Error promotion retroactively captures helper drafts | Staging/promotion machinery exists, but production helper staging is not wired. |
| Full-trace writer is available | It is not implemented. Recent, flight, values, and opt-in raw are the exact evidence paths. |
| Structural exhaustion gracefully sheds | Current live-ring cap hard-aborts; shed markers exist but the policy/ladder is not wired. |
| CCT storage is O(live boundaries) | Partition release does not currently reclaim all slab/node allocation; boundedness needs work. |
| Folded population counters remain exact at arbitrary scale | In-memory aggregate totals are **u64**, but several folded BCCT totals saturate to **u32::MAX** without a marker; **u32** histogram buckets instead wrap in release or panic in debug at overflow. Widening or explicit overflow state is required. |
| Every durable CAS root has a same-barrier durable pin | Pack sync precedes manifest append, but the append is not fsynced in that same barrier. |
| Retention independently prunes flight/trace budgets | Current pass prunes raw, whole history, whole sessions, and legacy profiles. |
| Flight dumps always pin referenced CIDs | GC recognizes such pin manifests, but current flight writer does not emit them. |
| **baml studio** is already gone | Both **baml studio** and **baml playground** currently exist; v1 target consolidates after parity. |
| Public DataFusion SQL is built | Current code still exposes BQL/**baml q**; DataFusion/**baml_query**/**baml query** are target work. |

### Target-design corrections

| Earlier proposal | Canonical resolution |
|---|---|
| Per-call table represents population | Population CCT and retained exact-instance relations remain separate (D1/D2). |
| Raw ClickHouse SQL or ClickHouse owns public semantics | DataFusion/BAML owns grammar/planning/outcomes; ClickHouse is a non-value hosted provider (D3/D6). |
| chDB is the required local architecture | Local physical provider is benchmark-owned per relation (D6). |
| Client-side/JSON AST **StudioQueryV1** | One portable SQL surface with private live RPC for tailing (D3/D10). |
| Store values/previews/scalars/path indexes in ClickHouse | No customer value content in ClickHouse; hydrate authorized S3/CAS values (D8). |
| Candidate count forces a background job | One bounded streaming path; count alone never changes semantics (D9). |
| Ordinary SQL follows a live tail | Every query binds a fixed catalog/generation/watermark/provider snapshot (D10). |
| Accepted S3 evidence expires by ordinary default retention | Indefinite hosted retention by default; only explicit erasure removes accepted evidence (D11). |
| Unavailable value is ordinary NULL/non-match | Preserve a typed unknown/evaluation state and incomplete outcome (D12/D13). |
| Prototype public `value_at`/`value_field`/typed-conversion chains | Ordinary SQL equality, comparison, and subscript syntax over virtual BAML values; DataFusion lowers to private internal expressions (D7). |
| User-defined query functions/plugins | Platform-owned allowlisted functions only (D14). |
| Only terminal rows are durable/queryable | Running/pending/so-far facts are part of the durable catalog (D15). |
| Unversioned public relation names | Versioned logical catalog such as **runs_v1**, **retained_calls_v1**, **cct_population_v1** (D16). |
| Prototype SHA-256 JSON store and all-call **function_calls** | Reuse canonical BLAKE3 DAG/CAS and honest population/retained grains. |

## Code surfaces checked

The implementation audit covered the current profiler record codec, rings, CCT engine, container/meta writers/readers, revision dictionary, value capture, canonical codec, pack/index store, history layout, retention/GC, fold reader, BQL/BQF1 surfaces, CLI command wiring, LSP/playground observability RPC, and dependency manifests. Links to the most useful source files are kept close to the relevant claims in [Profiler](03-profiler.md) and [Local artifacts](storage/local-artifacts.md).

The audit found no DataFusion or chDB dependency, no **baml_query** crate, no **baml query** command, and no Project Studio PostgreSQL/ClickHouse migrations on this branch as of the audit date.

## Prototype status

GitHub PR [BoundaryML/baml#4343](https://github.com/BoundaryML/baml/pull/4343) was an open, unmerged draft at the audit date. It is evidence that DataFusion/SQLite/hydration integration is feasible, not the canonical schema, store, budgeting model, or merge plan. The [Delivery plan](09-delivery-plan.md#starting-point) records the parts that must not be adopted unchanged.

## Post-audit implementation changes (2026-08-11, C1)

The current-implementation corrections above describe the branch **as
audited on 2026-08-10**. The C1 hardening pass then changed the following
audited facts; the component documents are updated, and this addendum
keeps the audit trail honest without rewriting it:

| Audited 2026-08-10 | Since C1 (2026-08-11) |
|---|---|
| Boundary manifest append not fsynced in the pack barrier | One crash-safe root-pin barrier: pack + manifest + directory fsyncs; dedupe trusts only provably durable chunks |
| Ring exhaustion hard-aborts; shed policy unwired | fail_run / abort_process / continue_incomplete implemented; shed persists as markers, boundary diagnostics, and BoundaryLoss |
| Folded counters saturate and histograms wrap without a marker | Saturation counted and persisted (SATURATED marker, diagnostics); histogram buckets saturate instead of wrapping |
| Corruption degradation not consistently persisted | Corrupt ranges counted, marked DEGRADED, reported in completion diagnostics, and surfaced through the fold reader |
| CLI drains values once synchronously at finish | CLI drains continuously off-thread (250 ms worker) with the durable commit barrier at stop |
| free_partition retains dead thread slabs | Dead thread slots recycle through a free list; the defer list is size-bounded |

Unchanged from the audit: BQL/**baml q** remain the compatibility query
surface, DataFusion/**baml_query**/**baml query** remain absent, the
full-trace writer stays deliberately absent, and helper staging/promotion
wiring stays deferred.

## Maintenance rule

When implementation changes:

1. verify behavior against code/tests;
2. update the affected component/storage document and status label;
3. update [Decision register](08-decisions.md) for semantic changes;
4. update [Deferred](10-deferred.md) if scope moves;
5. update [Delivery plan](09-delivery-plan.md) and the root ledger; and
6. record any newly superseded source or correction here.

Do not edit the historical [source archive](../archive/README.md) merely to make it agree with the canon. Its inconsistencies are part of the audit trail; path-only maintenance should be labeled as such.
