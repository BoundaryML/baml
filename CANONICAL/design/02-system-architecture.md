# System architecture

**Status:** Canonical v1 architecture. Local profiler/CAS/fold components are built; the public DataFusion query layer and hosted system are target work.

## Authority map

| Concern | Authority | Rebuildable? |
|---|---|---|
| Local exact evidence | Sealed **.baml** artifacts and canonical local CAS packs | No |
| Local live view | Runtime RAM tap plus committed artifact prefix | Yes |
| Local analytical acceleration | Provider-owned SQLite, Parquet, direct-artifact, or fold adapters | Yes |
| Local non-rebuildable control | **control.sqlite** for spool ownership, receipts, policy and pending operations | No |
| Hosted exact evidence | Accepted S3 artifacts/CAS plus service-authenticated receipts | No |
| Hosted transactional truth | PostgreSQL | No |
| Work delivery | SQS Standard pointers | Yes |
| Hosted resident analytics | ClickHouse non-value projections | Yes |
| Public query semantics | DataFusion/BAML SQL catalog, planner, functions, outcomes | Code/versioned contract |
| Live Studio UI | Private fold-engine-shaped RPC | Yes |

No projection becomes evidence. Losing ClickHouse, SQS, local SQLite, or Parquet must not lose accepted execution evidence.

## End-to-end topology

~~~mermaid
flowchart LR
  subgraph Host["BAML host"]
    VM["VM/runtime"]
    RINGS["Per-thread rings"]
    CONSUMER["Profiler consumer"]
    CCT["CCT + exact windows"]
    CAS["Canonical local CAS"]
    FILES[".baml artifacts"]
    FOLD["Fold engine / private RPC"]
    SPOOL["Spool + upload obligations"]
    TRANSPORT["Authorized transport"]
    LOCALQ["Local baml_query coordinator"]
  end

  subgraph Cloud["Hosted cell"]
    API["API / authorization"]
    HOSTQ["Hosted baml_query coordinator"]
    S3[("S3 artifacts + CAS + receipts")]
    PG[("PostgreSQL control ledger")]
    SQS[("SQS pointers")]
    PROJECTOR["Projector"]
    CH[("ClickHouse non-value facts")]
  end

  UI["Playground / CLI / agent"]

  VM --> RINGS --> CONSUMER
  CONSUMER --> CCT --> FILES
  CONSUMER --> CAS
  FILES --> FOLD
  CAS --> FOLD
  FILES --> LOCALQ
  CAS --> LOCALQ
  UI --> FOLD
  UI --> LOCALQ

  FILES -->|"sealed source ranges"| SPOOL
  CAS -->|"pack-bearing ranges"| SPOOL
  SPOOL --> TRANSPORT
  TRANSPORT -->|"authorize"| API
  TRANSPORT -->|"create-only PUT"| S3
  API -->|"commit + receipt state"| PG
  PG --> SQS --> PROJECTOR
  S3 --> PROJECTOR
  PROJECTOR --> CH
  UI --> API
  API --> HOSTQ
  HOSTQ -->|"resident subplan"| CH
  HOSTQ -. "only when values are required: authorized hydration" .-> S3
~~~

The local and hosted boxes instantiate the same backend-neutral **baml_query** contracts with different providers. Hosted work does not run inside the local CLI process, and resident-only queries never fetch S3 values.

## Local path

### Runtime capture

The producer writes fixed-width structural records into lock-free per-thread rings. It never performs filesystem, network, SQL, or value serialization work on call entry.

The background consumer:

- decodes each drained byte range once;
- maintains the CCT population tally;
- keeps the recent-call and flight-recorder windows;
- emits session and boundary artifacts;
- routes captured values to the separate value drain service; and
- records loss/degradation instead of silently truncating.

### Two read paths

The local product deliberately has two read paths:

1. **Private fold-engine RPC** for low-latency live/run-debugger views. It may read RAM taps, committed prefixes, and sealed snapshots.
2. **Public SQL** for portable questions. It binds a durable snapshot, uses backend-neutral table providers, and late-materializes values through the canonical CAS.

The UI RPC is not a public query language and may evolve with the UI. SQL is the versioned public integration contract.

### Local provider choice

SQLite, Parquet, and direct-artifact/fold providers are implementation or benchmark choices per logical relation. None is the architecture. **control.sqlite** remains a separate non-rebuildable control store and must not be confused with a query projection.

## Hosted ingest path

1. A host-specific drain adapter produces sealed, record-aligned source-range chunks.
2. A durable-spool host fsyncs the immutable chunk before seeking upload authorization.
3. The API returns exact, bounded, create-only object authorization.
4. The transport uploads one immutable object directly to S3.
5. A short PostgreSQL transaction commits the manifest and advances only contiguous commitment state.
6. The service anchors a deterministic receipt object in S3.
7. Only the minimum of contiguous committed and contiguous receipt-anchored watermarks is acknowledged to the client.
8. The outbox publishes a replaceable SQS pointer.
9. A fenced projector re-reads authoritative state, validates the object, writes deterministic non-value batches to ClickHouse, and advances its checkpoint only after visibility is verified.
10. Reconciliation repairs missing dispatch, queue expiry, orphan uploads, receipt gaps, stalled leases, ambiguous analytical writes, and projection drift.

The API does not download/decode customer chunks inside the commit transaction.

## Hosted query path

~~~mermaid
flowchart TD
  SQL["Portable SQL + authenticated request"]
  SNAP["Bind QueryScope + fixed snapshot"]
  PLAN["DataFusion logical plan"]
  PUSH["Proven-safe resident subplan"]
  CH["ClickHouse non-value facts"]
  CAND["Stream candidates + opaque value handles"]
  HYDRATE["Authorized batched S3/CAS hydration"]
  RESIDUAL["BAML value predicates / residual operators"]
  ROWS["Backpressured result batches"]
  OUTCOME["Mandatory typed query_outcome"]

  SQL --> SNAP --> PLAN --> PUSH --> CH --> CAND
  CAND -->|"resident-only query"| ROWS
  CAND -->|"value content referenced"| HYDRATE --> RESIDUAL --> ROWS
  ROWS --> OUTCOME
~~~

The planner pushes only semantics-preserving resident work. Resident-only queries can stream directly to output without reading S3. An inexact pushdown returns candidates that DataFusion rechecks. A final limit never moves below a hydrated predicate. Query-global budgets cover the full pipeline rather than resetting per Arrow batch.

## Component boundaries

### baml_query

Owns:

- public logical catalog and versioning;
- SQL parsing and logical planning;
- provider and capability contracts;
- trusted logical-to-physical mappings;
- BAML value functions;
- query-global budgets, cancellation, spill and backpressure;
- snapshot binding; and
- terminal query outcomes.

Must not depend on the runtime host, CLI, playground, AWS SDK, concrete SQLite store, or concrete ClickHouse client.

Core contracts:

~~~text
TableProvider/provider factory   logical relation -> physical source
ValueResolver                    authorized handle -> BAML value + availability
CapabilityRegistry              function/operator -> backend requirements
QueryScope                       tenant/project/environment + generation/barrier
Pushdown                         Exact | InexactCandidate | Unsupported
~~~

### Profiler crates

- **bex_events** — record/container formats, CCT, dictionary, canonical codec, CAS packs, GC/retention.
- **bex_engine** — VM-side call context and value capture.
- **bex_query** — current fold reader, run/value reads, BQL compatibility, UI frames.
- **baml_cli** — current run wiring and commands.
- **baml_lsp_server** — current playground observability RPC.

The future **baml_query** DataFusion crate is separate from the existing **bex_query** fold/BQL crate. Renaming or merging them is not a v1 semantic decision.

### Hosted roles

- **agent/transport** — discovery, spool, upload and local service;
- **api/query coordinator** — auth, control API, private RPC, SQL orchestration, authorized point reads;
- **dispatch/reconciliation** — outbox publication and invariant repair;
- **projector** — deterministic decode and non-value ClickHouse projection;
- **operations worker** — deletion, export, replay/reindex and later durable jobs;
- **migration roles** — isolated PostgreSQL and ClickHouse schema changes;
- **SPA** — static browser client with no database credentials.

These are responsibility boundaries. Multiple roles may share an image or initial service process without merging credentials.

## V1 topology constraints

- One region/cell per v1 project; no cross-cell ad-hoc SQL.
- No browser-to-database or PostgreSQL-to-ClickHouse CDC path.
- SQS is never authoritative.
- ClickHouse stores no customer value content.
- The public grammar is BAML/DataFusion SQL, not raw ClickHouse SQL.
- Hosted and local queries share logical semantics, not necessarily physical engines or DDL.
- Ordinary SQL sees durable running state at a fixed watermark; lower-latency tailing remains private RPC.
- Accepted hosted S3 evidence is immutable and indefinite by default.
