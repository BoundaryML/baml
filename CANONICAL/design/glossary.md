# Glossary

Terms in this document set are normative unless marked otherwise.

| Term | Meaning |
|---|---|
| **Accepted evidence** | An immutable hosted object whose manifest is committed in PostgreSQL and anchored by a verified service-authenticated receipt. An uploaded object alone is not accepted. |
| **Anchored watermark** | Highest contiguous stream sequence whose commitment has a verified receipt in S3. |
| **Artifact** | Versioned bytes emitted by the profiler/value pipeline, such as BCCT, BMET, raw/flight records, value records, dictionaries, or CAS packs. |
| **Artifact chunk** | One immutable, record-aligned source range uploaded as a bounded object. It is an upload unit, not a run. |
| **Availability** | Typed state explaining whether requested evidence can be evaluated: pending, available, not captured, omitted, redacted, lost, truncated, corrupt, unsupported, or a query/resource limit. |
| **BAML value** | A typed value in BAML’s canonical value model; it is not interchangeable with generic JSON. |
| **BCCT** | Checksummed block container used by live CCT segments and folded CCT snapshots. |
| **BMET** | Append-only framed metadata container used by session and boundary metadata. |
| **Boundary** | Runtime-owned root execution identity for one BAML invocation; the root of a run’s local causal evidence. |
| **BQF1** | Existing compatibility framing for current query responses. It is not the target public SQL contract. |
| **BQL** | Existing BAML query language used by **baml q**. It is superseded after the SQL replacement reaches parity. |
| **CAS** | Content-addressed store for canonical BAML value DAG nodes/chunks, locally in **.bamlpack** files and hosted inside accepted pack-bearing artifacts. |
| **Candidate pushdown** | Inexact resident filtering that may reduce rows but requires DataFusion to recheck the original predicate. |
| **Capture policy** | Rules deciding which optional values/logs/exact evidence are retained. Exact v1 defaults are a policy freeze item. |
| **Causal run** | See **Run**. It may contain several logical threads and spawned operations but does not invent exact cross-process ordering. |
| **CCT** | Calling-context tree: population aggregate keyed by calling path, with counts, timing, histograms, LLM totals, and loss/degradation state. |
| **Cell** | Hosted failure/routing domain containing cell-local control, evidence, projection, and query responsibilities. V1 places a project in one cell at a time. |
| **CID** | BLAKE3-based canonical value/node identity encoded as **bamlv_1_…**. It is versioned by codec/domain. |
| **Committed watermark** | Highest contiguous stream sequence committed in PostgreSQL. |
| **Committed prefix** | Longest checksummed portion of an append-only artifact that a reader can safely observe after a torn write. |
| **Complete query** | A query that reached its terminal outcome within its bound snapshot and budgets. It can still be *incomplete as evidence* if value evaluations were unavailable; callers must inspect the outcome. |
| **control.sqlite** | Proposed non-rebuildable local control database for spool ownership, upload obligations, receipts/watermarks, policies, and pending operations. It is not an analytical provider cache. |
| **DataFusion/BAML SQL** | Target public SQL surface: DataFusion relational semantics plus platform-owned BAML value/path functions, provider capabilities, budgets, snapshots, and outcomes. |
| **Definition key** | Stable semantic function identity used across revisions. A rename intentionally changes it. |
| **Deferred** | Deliberately outside v1 or awaiting a separate product/policy decision. It does not mean an already-settled v1 implementation gap. |
| **Degraded** | Evidence whose attribution or completeness was affected by declared loss/corruption/budget state. Degraded is never silently presented as complete. |
| **Direct-artifact provider** | Public-query table provider that reads/folds canonical profiler artifacts without first materializing a separate database table. |
| **Durability watermark** | Client-reclaimable hosted sequence: the minimum of contiguous committed and contiguous anchored watermarks. |
| **Evidence** | Canonical or durably committed information used to explain execution. Projections and queues are not evidence authorities. |
| **Exact pushdown** | Provider pushdown proven to preserve the logical predicate/operator semantics exactly. |
| **Exact window / tape** | Bounded individually retained calls/events such as recent calls or a flight dump. It does not represent the full call population. |
| **Flight recorder** | Bounded circular exact-event buffer dumped on triggers/manual request. Current implementation is consumer-global and shared across engines. |
| **Fold engine** | Existing reader/aggregation layer over local artifacts and live state. Its RPC is private to the playground and optimized for debugging. |
| **Function ID** | Dense **u32** compiler/profiler identity meaningful only with its revision; real functions begin at 16. Not a cross-revision key. |
| **Generation** | Versioned physical projection/provider universe. A query binds exactly one generation; this is independent of logical catalog version. |
| **Grain** | What one row represents. Core grains include run, population CCT path, retained call, exact window, loss fact, function, and revision. |
| **Hosted value handle** | Authorization-gated opaque provider reference used to locate a canonical value in S3/CAS. Its physical representation and equality semantics are unresolved. |
| **Hydrated column/operator** | Logical value data or predicate that requires resolving canonical value content outside the resident analytical store. |
| **Integrity state** | Independent classification such as unverified, verified, truncated, corrupt, conflicting, or quarantined. |
| **Lane** | Ordered hosted ingest/projection routing subdivision inside a cell. |
| **Ledger date** | Object/ledger routing partition fixed at first authorization. It is not part of artifact-chunk identity. |
| **Logical catalog** | Versioned public relations, columns, types, grains, identities, availability, and semantics exposed to users. Physical schemas remain private mappings. |
| **Observation** | Individually discoverable retained operation (run, retained call, model attempt, tool/resource operation) with sufficient emitted identity and lifecycle evidence. Not every call becomes one. |
| **Opaque provider handle** | Private, non-bearer locator carried by resident metadata and resolved only within an authorized QueryScope. |
| **Population grain / tally** | Complete-at-snapshot aggregate evidence to which every runtime call contributes, primarily through the CCT. |
| **Program snapshot** | Content identity of BAML source/schema/compiler inputs explaining a run. Release/git/build/service labels are dimensions, not the identity itself. |
| **Projected-through** | Durable evidence barrier through which a projection is known to represent accepted input. |
| **Projection** | Rebuildable analytical or UI acceleration derived from canonical evidence, such as ClickHouse facts or a local provider index. |
| **Query outcome** | Mandatory out-of-band terminal record describing completion, snapshot, budget/cancellation/error, evidence availability, and result completeness. It is not a SQL data row. |
| **QueryScope** | Immutable authorization, catalog, generation, watermark, provider-snapshot, policy, budget, deadline, and cancellation context bound before planning. |
| **Raw firehose** | Opt-in traffic-proportional structural event stream used as a profiler correctness oracle. Not default capture. |
| **Receipt** | Deterministic service-authenticated S3 object anchoring an accepted manifest set so commitment can be restored/reconciled. |
| **Reconstruct** | Decode canonical evidence again and compare semantic results/hashes; does not execute the user program. |
| **Reindex / reproject** | Rebuild a derived provider/projection from a fixed canonical evidence barrier. |
| **Resident column/operator** | Logical data or work available in the local provider or ClickHouse without fetching customer value content. |
| **Retained-instance grain** | Individually discoverable calls/events retained by policy, exact windows, promotion, or another explicit mechanism. Counting these rows is not a population count. |
| **Revision dictionary** | Per-revision protobuf mapping dense function IDs to names, source spans, kind/origin, definition key, content hash, and capture flags. |
| **Run** | One runtime-owned causal graph rooted at a BAML boundary, including structural, value, loss, and program identity. Cross-process relations remain explicit rather than inventing one clock. |
| **Running/so-far fact** | Durably committed nonterminal row with explicit pending state and snapshot-relative totals/duration. It is queryable, not silently omitted. |
| **S3/CAS hydration** | Authorized, bounded resolution and canonical decoding of distinct value handles after resident candidate filtering. |
| **Snapshot** | Fixed catalog version, projection generation, projected-through barrier, scope, and provider handles bound for one ordinary SQL query. |
| **Source artifact** | Exact runtime-produced file or logical artifact from which source-range chunks are made. |
| **Spool** | Host-local durable immutable upload obligations and bytes, tracked separately from rebuildable analytics. |
| **Stale open** | Observability classification for a run/stream with no recent committed progress; not an invented execution terminal state. |
| **Studio** | Initiative/product-experience name. The v1 local browser command remains **baml playground** rather than adding a separate Studio shell. |
| **Structural completeness** | Whether call/thread/event structure is complete, open, gapped/incomplete, diagnostic, or stale-open at a snapshot. |
| **Tally** | See **Population grain**. |
| **Tape** | See **Exact window**. |
| **Typed unknown** | Row/evaluation state used when a value predicate cannot be decided. It is distinct from SQL NULL, false, and a missing row. |
| **Unavailable** | Evidence cannot currently be supplied/evaluated for a typed reason. It must contribute to terminal completeness accounting. |
| **ValueResolver** | Backend-neutral query contract resolving an authorized provider handle to canonical BAML value content plus availability under global budgets/cancellation. |
| **Versioned logical relation** | Public catalog name such as **runs_v1**, **cct_population_v1**, or **retained_calls_v1** whose semantics do not drift silently. |
| **Watermark** | Monotonic evidence/projection progress barrier. The adjective—committed, anchored, projected-through, client durability—must always be stated. |

## Similar terms that must not be collapsed

- **Run status** is not **structural completeness**, **value availability**, **integrity**, **projection**, or **retention** state.
- **Population** is not the set of **retained instances**.
- **Uploaded** is not **committed**, **anchored**, or **accepted**.
- **Query completed** is not the same as “all requested evidence was available.”
- **Catalog version** is not **physical projection generation**, artifact format version, or program revision.
- **Definition key** is not revision-local **function ID**.
- **S3 canonical evidence** is not a **ClickHouse projection** or an SQS notification.
- **Reopen/reconstruct/reindex** do not execute user code; **rerun** does and is deferred.
