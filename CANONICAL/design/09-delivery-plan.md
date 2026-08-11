# Delivery plan

**Status:** Canonical implementation sequence. Milestones are ordered by dependency and evidence gates, not calendar promises.

## Starting point

The current branch already contains a substantial local substrate, but not the target public SQL or hosted system.

| Area | Verified current state | Consequence |
|---|---|---|
| Profiler records, rings, CCT, dictionaries, BMET/BCCT, recent/flight/raw capture | Built | Reuse and harden; do not replace with per-call rows |
| Canonical BAML value DAG, packs, reader, GC/retention | Built | One codec/CID/store model for every query provider |
| Boundary history and fold reader | Built | Supplies local run/value inspection and direct-artifact providers |
| Playground observability UI/RPC | Built core views | Evolve into the v1 experience; keep private live RPC separate from SQL |
| **baml q**, BQL parser/evaluator, BQF1 | Built compatibility surface | Delete only after SQL/CLI/API parity and migration |
| **baml studio** and **baml playground** commands | Both currently exist | Consolidate only after replacement behavior ships |
| DataFusion, **baml_query**, **baml query** | Not present | First public-query implementation dependency |
| Project Studio PostgreSQL/ClickHouse schemas | Not present | Requires schema freeze and migrations |
| New artifact/CAS S3 uploader and receipt protocol | Not present | Legacy **tracingv2** publisher is not this path |
| Hosted projector/query coordinator | Not present | Build after commitment and schema contracts |

An open prototype PR demonstrates useful DataFusion, SQLite, and hydration techniques, but is not merged and is not the v1 physical architecture. In particular, do not adopt its SHA-256 JSON value store, all-call **function_calls**, NULL-only availability, per-batch budgets, or SQLite-as-invariant choices.

## Critical path

~~~mermaid
flowchart LR
  C0["C0 Canonical contract"]
  C1["C1 Profiler correctness"]
  Q1["Q1 Catalog + baml_query core"]
  Q2["Q2 Local providers + CLI"]
  Q3["Q3 Conformance + parity"]
  U1["U1 Local Studio P0"]
  H1["H1 Hosted evidence plane"]
  H2["H2 Projection + hosted query"]
  R1["R1 Release hardening"]

  C0 --> C1
  C0 --> Q1 --> Q2 --> Q3
  C1 --> Q2
  Q2 --> U1
  C1 --> H1 --> H2
  Q3 --> H2
  U1 --> R1
  H2 --> R1
~~~

Profiler hardening, SQL-core work, and hosted schema work can proceed in parallel after the canonical contract, but release waits for shared conformance and evidence invariants.

## C0 — Canonical contract and source reconciliation

Deliverables:

- this navigable canonical design set;
- explicit current/target/deferred labels;
- settled D1–D16 decisions and X1–X4 open choices;
- complete known local/S3/PostgreSQL/ClickHouse inventories; and
- source disposition and correction ledger.

Gate:

- every archived source has a disposition;
- internal links and terminology validate;
- no target feature is described as already built; and
- the archive move/path normalization preserves the substantive source record.

## C1 — Profiler correctness and durability

Required work:

- make pack durability and boundary/flight/upload root-pin durability one crash-safe protocol;
- bound or reclaim partition/slab/node memory honestly;
- widen folded counters/histograms or persist explicit overflow/saturation state;
- select and implement the structural-exhaustion policy instead of unconditional process abort;
- wire continuous off-thread value draining where host guarantees require it;
- wire speculative helper staging/promotion or remove the unshipped promise;
- decide whether full trace is required for v1, then implement only if selected;
- reconcile flight-dump CID pins with GC;
- expose capture/loss/durability state consistently through fold/RPC/artifacts; and
- keep the opt-in raw stream as a correctness oracle.

Gate:

- hot-path benchmarks meet allocated budgets;
- crash/torn-write, disk-full, ring-exhaustion, recursion, thread-churn, retention, and GC-race suites pass;
- no acknowledged/durable root can be collected; and
- every incomplete path produces explicit evidence state.

Exact capture/redaction, quota, retention, and index-policy values are X1 policy work. The implementation must be policy-driven without inventing defaults in the public contract.

## Q1 — Freeze catalog v1 and build the query core

Deliverables:

- a backend-neutral **baml_query** crate;
- the complete versioned logical catalog, including **runs_v1**, **cct_population_v1**, and **retained_calls_v1**;
- exact Arrow/SQL types, nullability, keys, grain, availability, provenance, and snapshot fields;
- the platform-owned BAML value/path function catalog;
- the typed row-level unavailable/unknown carrier;
- QueryScope, snapshot, provider, ValueResolver, and capability contracts;
- query-global budgets, cancellation, memory/spill, backpressure, and outcome types; and
- planning-time **E_BACKEND_CAPABILITY** behavior.

Gate:

- grammar/catalog golden tests;
- planning tests for every capability and forbidden construct;
- fixed-snapshot and running/pending semantics tests;
- every SQL stream has exactly one terminal **query_outcome**, including planning/execution failure, budget exhaustion, and cancellation; and
- no dependency from the core to CLI, runtime host, AWS SDK, concrete SQLite, or concrete ClickHouse client.

The exact public function spellings and unavailable carrier are freeze work, not product deferrals.

## Q2 — Local providers and command surface

Deliverables:

- provider selection per logical relation using direct artifacts/fold, rebuildable SQLite, Parquet, or a measured mixture;
- one local canonical-CAS ValueResolver;
- generation/manifest-bound ordinary query snapshots;
- **baml query --schema** and streaming JSON/JSONL/Arrow-friendly output;
- stable machine-readable run/value/source commands under **baml playground**; and
- benchmark corpus covering empty, ordinary, large, gapped, running, corrupt, and value-heavy projects.

Gate:

- deleting provider state and rebuilding produces identical normalized results/outcomes;
- predicate/limit/aggregate behavior matches a no-pushdown reference executor;
- candidate hydration is batched, deduplicated, bounded, cancelled, and backpressured;
- no second value codec/CID/store exists; and
- local qualification targets pass on the published corpus.

Provider selection is benchmark-owned. SQLite, Parquet, or direct artifacts are not user-visible architectural commitments.

## Q3 — Compatibility parity and consolidation

Deliverables:

- a corpus mapping every supported BQL/BQF1/UI question to the SQL catalog or private live RPC;
- SQL/API/CLI/fold agreement on identity, grain, availability, running rows, loss, and terminal outcomes;
- migration notes for saved BQL/query clients;
- deletion of BQL/StudioQueryV1 only after parity; and
- one supported local browser entry point: **baml playground**.

Gate:

- golden comparison passes for all supported legacy behaviors;
- any intentional incompatibility is documented and versioned;
- users have a replacement before old commands/endpoints are removed; and
- no raw ClickHouse dialect or second public query language remains.

## U1 — Local Studio P0 experience

Required user flows:

- open recent, running, succeeded, failed, cancelled, and abandoned runs;
- filter by durable metadata and see whether results are population or retained-instance grain;
- inspect the causal run graph, threads, CCT/tree/flame/timeline, source, errors, values, logs, and exact-window evidence;
- see pending, unavailable, degraded, corrupt, and projected-through states without guessing;
- issue/copy portable SQL and inspect the terminal outcome;
- provide stable machine-readable commands for an agent; and
- preserve virtualization and bounded detail reads for large runs.

Gate:

- UI, CLI, private RPC, and SQL conformance on the same fixtures;
- stale/out-of-order live frames cannot overwrite newer terminal state;
- no value is fetched until selected or needed by a query;
- accessibility, empty/error/degraded states, and large-run performance pass; and
- “Studio” does not create a second command/product shell.

The exact default landing grain and final screen layout remain X2 product work. Build the information architecture without freezing an unsupported choice early.

## H1 — Hosted evidence and control plane

Deliverables:

- versioned ArtifactChunkEnvelopeV1 codec and hostile decode limits;
- host adapters, durable spool, create-only S3 authorization, immutable upload, commitment, deterministic receipt, and contiguous anchored watermark;
- PostgreSQL migrations for the known inventory plus frozen missing keys/types/constraints;
- forced RLS, scoped roles, idempotency, conflict/quarantine, audit, deletion, and legal-hold state;
- outbox/SQS pointer delivery and reconciliation; and
- real-provider S3 qualification.

Gate:

- every ambiguous upload/commit/receipt outcome resolves safely;
- an acknowledged chunk survives host deletion and PostgreSQL restore/reconciliation;
- a later sequence never masks an earlier gap;
- cross-tenant and credential-scope suites pass;
- accepted evidence cannot be aged out by routine maintenance; and
- the legacy **tracingv2** publisher is neither mislabeled nor silently reused as this protocol.

## H2 — Projection and hosted query

Deliverables:

- fenced deterministic projector and generation/checkpoint workflow;
- ClickHouse migrations containing only non-value resident facts;
- duplicate/conflict-safe serving mappings independent of background merge timing;
- DataFusion ClickHouse provider for exact/inexact/unsupported pushdown;
- authorized batched S3/CAS ValueResolver using opaque provider handles;
- query coordinator admission, cancellation, audit, workload isolation, and terminal outcome; and
- projection rebuild, dual-generation cutover, rollback, and validation tooling.

Gate:

- empty-ClickHouse rebuild from accepted S3/PG authority passes;
- local versus hosted and pushdown-on versus pushdown-off conformance passes;
- value content cannot be found in ClickHouse tables, logs, caches, or query-scoped leftovers;
- row-policy, grants, settings caps, and scope-forgery suites pass;
- queries remain correct across delayed/running projections and unavailable values; and
- qualification targets pass with projector recovery headroom reserved.

X4 must be resolved before freezing opaque handle columns. It must not be “resolved” accidentally by persisting a raw tenant CID in ClickHouse.

## R1 — Release hardening

Release requires:

- the complete [security and reliability validation](07-security-and-reliability.md#v1-validation-and-release-gates);
- measured capacity envelopes and overload/admission behavior;
- dashboards and alerts for evidence, projection, query, receipt, quarantine, and erasure invariants;
- backup/restore, ClickHouse rebuild, generation rollback, and receipt-import drills;
- canary, rollback, incident, tenant-isolation, data-loss, and deletion runbooks;
- supported-version and migration policy for artifacts, catalog, envelope, PostgreSQL, and ClickHouse; and
- documentation/CLI/schema snapshots that match the shipped binaries.

## Deferred tracks

Do not place these on the v1 critical path: durable background query jobs, local/hosted federation, value-content indexes, rerun/test creation, arbitrary user query code, multi-cell ad-hoc SQL, collaboration/billing/BYOK/enterprise packaging, or a default hosted expiry window. The full register is [Deferred](10-deferred.md).

## Change-control rule

A milestone may change physical implementation after benchmarks, but it may not silently change grain, identity, availability, authorization, snapshot, durability, or outcome semantics. Such a change requires an explicit decision update, conformance fixtures, migration impact, and corresponding edits to the ledger and component documents.
