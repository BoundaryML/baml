# Query Architecture Decision Log

**Status:** Working decision log. This file records decisions as they are resolved one at a time. It is not yet a replacement for `PLAN.md`, `profiling-design.md`, or `studio-design.md`; those documents will be reconciled after the remaining decisions close.

## Decision process

- Ask and resolve exactly one architecture question at a time.
- Record settled semantics separately from implementation recommendations and benchmark-owned choices.
- Do not let a physical engine choice silently redefine the public query contract.

## Settled decisions

### D1 — Population aggregates plus retained exact instances

Every call contributes to the population-true CCT aggregates. Only calls retained by capture policy, exact-evidence windows, promotion, or another explicit retention mechanism are individually discoverable.

Consequences:

- No default traffic-proportional row for every BAML call.
- `*_population_vN` views answer complete population questions.
- Retained call/value rows answer questions about recorded instances only.
- A search over call values means “matches among retained and indexed instances,” not population prevalence.
- Search results must expose unavailable, capture-disabled, redacted, lost, and not-indexed evidence where relevant.

### D2 — All useful value-query grains remain available

Occurrence-grain, unique-root/CID-grain, and scalar/path-grain queries are all legitimate operations over the same deduplicated value model. They do not require different storage contracts or different deduplication schemes.

- **Occurrence grain:** which retained executions referenced a matching value.
- **Root/CID grain:** which distinct captured values matched and how often they occurred.
- **Scalar/path grain:** which exact structured leaves matched and at which argument/field path.

Which grain Studio displays by default is a later UX choice, not an architecture decision.

### D3 — DataFusion coordinates the public query; ClickHouse remains the hosted analytical engine

“DataFusion everywhere” means that the DataFusion/BAML layer owns the public SQL contract, logical planning, BAML-specific functions, and residual execution locally and in cloud. It does **not** mean that DataFusion must replace every physical database.

Cloud ownership:

```text
canonical exact evidence and value bodies   S3 artifacts + canonical CAS
hosted non-value analytical projections     ClickHouse
public SQL semantics and logical planning    DataFusion/BAML SQL
hosted resident relational execution         ClickHouse, behind DataFusion
body hydration and BAML residual execution   DataFusion + authorized S3/CAS reader
```

ClickHouse is the primary hosted analytical store and remote execution backend for **non-value resident facts**. It owns hot metadata/population projections, filters, joins, grouping, ordering, and safe limits. It remains rebuildable from canonical evidence.

ClickHouse does not store customer value content or value-derived search material: no argument/return/error bodies, decoded JSON, scalar leaves, previews, path/value rows, or plaintext text index. It may store the opaque tenant-scoped root references and availability/occurrence metadata required to associate a retained call with the canonical value in S3; those references are handles, not bodies, and are authorization-gated.

DataFusion pushes the largest non-value resident subplan to ClickHouse only when the translation is proven semantics-preserving. DataFusion retains all value-content work, including recursive CAS hydration, `value_at`/`value_field`/`contains` predicates, typed scalar/path evaluation, allowlisted asynchronous UDFs, joins to temporary external relations, and limits or aggregates affected by those residual predicates.

Example mixed execution:

```text
ClickHouse: tenant/time/function filters -> call/root IDs + value CIDs
DataFusion: distinct-CID hydration -> BAML predicate -> final LIMIT
```

A final `LIMIT` cannot be pushed below a hydrated residual predicate because ClickHouse does not yet know which candidates survive.

### D4 — Backend-specific functions fail explicitly where unavailable

The portable BAML SQL catalog must retain the same meaning locally and hosted. A known ClickHouse-only function may be exposed as an explicit backend capability. Executing such a query locally must fail during planning, before reading data, with a typed backend-capability error rather than generic invalid SQL or silent semantic substitution.

Example:

```text
E_BACKEND_CAPABILITY
function: clickhouse.quantileExact
required_backend: clickhouse
current_backend: local
```

A local query must never silently upload data or fall back to hosted execution.

### D5 — ClickHouse extensions are allowlisted within BAML/DataFusion SQL

V1 exposes only an allowlisted set of ClickHouse functions whose call syntax fits the BAML/DataFusion SQL grammar. Each extension declares the backend capability it requires. Unsupported local execution fails during planning with the typed capability error from D4.

V1 does not ship:

- a second ClickHouse SQL parser;
- a raw `--dialect clickhouse` passthrough;
- unrestricted access to ClickHouse physical tables;
- silent routing based on which syntax a statement happens to contain.

This keeps one public grammar and makes the portability boundary explicit while still allowing selected hosted acceleration.

### D6 — Backend-neutral query crate and trusted provider mappings

`baml_query` is a separate DataFusion-based coordination crate. It owns the public logical catalog, planning, capability metadata, query-global budgets/cancellation, pushdown contracts, and UDF registration. It does not depend on a concrete local or hosted storage engine, the BAML runtime host, CLI, playground, AWS SDK, or ClickHouse client.

Allowed dependencies are generic query machinery such as DataFusion/Arrow/async primitives and small leaf semantic contracts. Avoiding those dependencies must not cause `baml_query` to invent another JSON codec, CID space, or BAML value model.

Logical tables and columns map through trusted code- or migration-owned provider definitions to arbitrary physical table/column names. A mapping includes more than names:

- logical type and nullability;
- grain and identity/key scope;
- evidence/availability columns;
- resident versus hydrated status;
- required backend capabilities;
- schema/view version.

Physical names may change freely behind a provider. Public logical names are versioned API and require compatibility treatment when changed. Untrusted SQL cannot choose or alter physical mappings.

The core provider contracts include:

```text
TableProvider/provider factory   logical schema -> physical source
ValueResolver                    authorized CID -> BAML value + availability
CapabilityRegistry              function/operator -> backend requirements
QueryScope                       tenant/project/environment + generation/barrier
Pushdown                         Exact | InexactCandidate | Unsupported
```

Queries spanning providers bind one stable snapshot/evidence barrier. An inexact pushed predicate may only generate candidates; DataFusion rechecks the normative predicate before returning results.

Local execution is therefore:

```text
canonical evidence       .baml artifacts + existing canonical CAS packs
public SQL/planner       DataFusion/BAML SQL in baml_query
resident providers       rebuildable SQLite, Parquet, and/or direct-artifact adapters
hydration                existing canonical CAS through ValueResolver
live playground UI       existing fold engine and private RPC
```

SQLite is the first practical adapter demonstrated by PR #4343, not an architectural invariant. SQLite versus Parquet versus direct artifact/fold providers is benchmark- and implementation-owned and may differ by logical table. `control.sqlite` remains separate non-rebuildable control state.

PR #4343’s prototype physical contracts are not adopted: no loose SHA-256 JSON value store, no apparently population-true all-call `function_calls` table, no NULL-only evidence model, and no per-batch limits masquerading as query-global budgets.

### D7 — Typed scalar/path predicates remain part of the logical surface

The public BAML/DataFusion surface supports canonical path navigation plus typed predicates rather than forcing every primitive through text search. This is evaluated over values hydrated from the canonical S3/local CAS, not over a standing ClickHouse scalar table. Illustratively:

```sql
SELECT call_id
FROM retained_calls_v1
WHERE baml_value_int(
        baml_value_at_path(args, baml_path('arg[0].customer.age'))
      ) >= 30;
```

A scoped or deferred DataFusion provider may expose hydrated value nodes relationally, but `value_nodes_v1` is not a persistent ClickHouse value-content view. Exact function and virtual-relation names remain to be frozen.

### D8 — No customer value content is stored in ClickHouse

Canonical and decoded values live in S3/CAS in cloud and the canonical local CAS locally. ClickHouse contains non-value analytical facts plus only the opaque tenant-scoped value references and occurrence/availability metadata necessary to locate and authorize the corresponding S3 value.

Specifically, ClickHouse does not persist:

- argument, return, error, or log bodies;
- decoded JSON or BAML values;
- scalar leaf values or bounded previews;
- value-path/scalar rows;
- plaintext/token/ngram search indexes derived from customer values.

Consequences:

- every predicate that inspects value content hydrates S3/CAS;
- ClickHouse resident predicates should reduce the candidate root references first;
- small candidate sets may hydrate during an interactive query;
- large candidate sets require a checkpointed deferred S3/CAS scan;
- a future external value index would be a separate explicitly decided system and cannot be introduced as an implicit ClickHouse projection.

### D9 — One streaming SQL execution path; deferred scans are not required for scale

DataFusion executes valid value queries through one streaming path:

```text
stream resident candidates from ClickHouse
-> hydrate distinct S3/CAS values in bounded batches
-> evaluate BAML residual predicates/operators
-> stream results with backpressure
-> continue until complete, cancelled, or a configured budget is exhausted
```

The architecture imposes no fixed candidate-count ceiling and does not classify a query as deferred merely because it may touch many values. Simple scan/filter/projection queries can process arbitrarily large finite candidate sets without materializing them all at once.

Sane configurable limits remain necessary for S3 bytes/requests, decoded bytes/nodes, CPU/time, memory/spill, result bytes, concurrency, and tenant fairness. Hitting a limit terminates the stream with a typed `E_QUERY_BUDGET_EXCEEDED`; any rows already streamed are explicitly incomplete and never reported as a successful complete result. Callers may rerun with a higher permitted budget.

Implementation requirements include streaming `RecordBatch` output rather than `collect()`, bounded hydration caching, query-global accounting, async batched/range S3 reads, DataFusion memory-pool/spill integration, cancellation, and output backpressure. Operators such as global sort, high-cardinality aggregation, windows, and joins may spill or consume larger bounded state even though hydration itself is lazy.

### D10 — Ordinary SQL binds a fixed query snapshot

At query start, DataFusion binds a snapshot containing at least the logical catalog/view version, projection generation, durable projected-through watermark/evidence barrier, tenant/project/environment scope, and the provider-specific snapshot handles needed to enumerate one stable candidate universe. Rows committed or projected after that barrier are not visible to the query.

All ClickHouse candidate batches, local providers, and S3/CAS hydration in the query use that same snapshot. This prevents duplicates, omissions, changing aggregates, and non-termination while ingestion continues. A separate tail/live surface may intentionally follow later data; ordinary SQL does not.

### D11 — Committed S3 evidence is retained indefinitely by default

Accepted artifacts and canonical CAS content are immutable and have no automatic age- or size-based deletion in the default hosted policy. The query system, projector, compactor, and ordinary maintenance cannot opportunistically remove committed customer evidence.

Deletion occurs only through an explicit authorized erasure request at a supported customer/project/run scope. Logical access is denied first, active queries in that scope are cancelled, and physical S3/CAS deletion proceeds asynchronously with verification. A query snapshot never preserves revoked authorization.

Cleanup of uncommitted orphan uploads, abandoned multipart objects, expired temporary query/export results, and obsolete rebuildable projections is separate: those objects are not accepted canonical customer evidence. Future customer-configured retention windows remain an explicit deferred product/policy decision.

Because default committed evidence is not reclaimed during ordinary operation, normal query snapshots do not require a reader lease merely to protect against age-based S3 retention.

### D12 — Data-level value failures preserve rows and mark the result incomplete

A missing, corrupt, unsupported, redacted, not-captured, or otherwise data-level unavailable S3/CAS value does not poison an entire cohort query. The affected row/predicate evaluation carries a typed availability reason, remains unknown rather than becoming an ordinary SQL `NULL` or non-match, and the query result is explicitly marked incomplete with reconciled counts/reasons.

Query-wide failures remain distinct:

- insufficient value-read authorization fails before execution;
- general S3/CAS dependency unavailability fails as retryable;
- query budget exhaustion terminates the stream as explicitly incomplete;
- cancellation terminates the query.

A future strict mode may request fail-on-any-unevaluable-row, but it is not the default.

### D13 — Every SQL stream ends with a mandatory typed query outcome

SQL schemas and rows remain unchanged. The transport ends with an out-of-band `query_outcome` trailer/envelope carrying the bound snapshot, whether execution completed, `complete | incomplete` result state, and—when values were evaluated—attempted/available/unavailable counts reconciled by typed reason.

A successful value query therefore returns ordinary rows followed by a small outcome such as:

```json
{
  "queryCompleted": true,
  "resultState": "complete",
  "snapshot": {"projectedThrough": "..."},
  "valueEvaluations": {
    "attempted": 12,
    "available": 12,
    "unavailable": 0
  }
}
```

Non-value queries still return a minimal successful outcome without `valueEvaluations`. Human CLI rows remain on stdout and a compact outcome goes to stderr; structured streaming uses a typed terminal control frame, never a synthetic SQL row. If the stream ends without its terminal outcome, the caller cannot treat it as a complete successful query.

This is narrow execution/evidence metadata collected by the providers and hydrator, not a second query language or a restoration of coverage modes.

### D14 — V1 exposes only the platform-owned SQL function catalog

V1 has no user-defined query-function surface. Users cannot register BAML functions as SQL UDFs, use `CREATE FUNCTION`, upload plugins, or execute arbitrary code through a query.

The available functions are only:

1. the ordinary SQL/DataFusion functions supported by the public BAML SQL surface;
2. BAML value-navigation and predicate functions implemented by the platform for both local and cloud execution; and
3. explicitly allowlisted ClickHouse functions when the query runs against the hosted ClickHouse backend.

The third group is cloud-only and follows D4/D5: planning the same function locally returns typed `E_BACKEND_CAPABILITY`; there is no silent fallback and no second ClickHouse SQL dialect. Whether an implementation happens to use DataFusion UDF machinery or asynchronous internals is not part of the user-facing contract. The architecture does not use a separate deterministic/side-effect classification system—the catalog is small and owned by us, and each shipped function has specified semantics and capabilities.

### D15 — Ordinary SQL includes durably committed running state

A fixed query snapshot includes both terminal executions and the durably committed facts projected for executions still running at its watermark. SQL never reads uncommitted or RAM-only state.

Running rows expose explicit lifecycle/evidence semantics:

- `execution_state = 'running'` (or the frozen equivalent);
- not-yet-produced return/error/end facts are `pending`, not ordinary `NULL`, missing, or corrupt;
- counters are explicitly snapshot/so-far values rather than terminal totals;
- a later query snapshot may observe more facts or a terminal state.

A query can be `complete` relative to its bound snapshot even though some execution rows in that snapshot are still `running`; query completeness and execution terminality are separate. Fold-engine/private-RPC tail surfaces remain available for lower-latency live inspection beyond the durable projection watermark.

### D16 — Preserve the existing versioned logical SQL catalog

This was already locked by the canonical Studio contract and did not require a new question. Public query requests select a catalog version and use stable versioned logical relations such as `runs_v1`, `retained_calls_v1`, and `cct_population_v1`. Physical mappings may change without changing that contract. Saved queries pin the version they were authored against; any convenience alias policy is non-blocking UX detail.

## Remaining architecture blockers

None. D1–D16 are sufficient to reconcile the three canonical plan documents. The items below remain deliberately deferred and are not prerequisites for that reconciliation.

## Explicit deferred decisions

### X1 — Capture, value-access, and query-budget policy

Defer the actual policy values, including:

- capture-exact versus summary/omitted defaults by environment and value role;
- path allowlists, denylists, redaction, and sensitive-field handling;
- maximum hydration depth, nodes, array elements, string bytes, decoded bytes, S3 requests, and bytes per query;
- default and maximum query duration, memory/spill, output, concurrency, and tenant quotas;
- any future opt-in retention-window product, plus deletion workflow details and policy-version migration;
- whether a future value-search index outside ClickHouse is ever introduced, and its separate security/deletion contract.

The architecture must preserve policy/version/evidence columns and explicit `not_captured`, `redacted`, `truncated`, `query_budget_exhausted`, and—if a future index exists—`not_indexed` states so these choices can be made later without changing query meaning.

### X2 — Studio search-result presentation

Defer which supported query grain Studio displays by default.

### X3 — Durable background/checkpointed query operations

Defer a separate operation mode for queries that should survive client disconnects or worker restarts, expose durable progress, run in a low-priority queue, or persist large result sets. This is an operational product capability, not a prerequisite for executing a large value query correctly.

### X4 — Hosted opaque S3 value-reference representation

Defer whether ClickHouse stores raw canonical CIDs, deterministic tenant-scoped opaque tokens, or random occurrence references. Before resolution, no schema or API may assume cross-tenant/local-hosted comparability, stable public token syntax, key-rotation behavior, or equality-query availability. ClickHouse still requires some authorized non-content handle, directly or through a separate lookup, to associate retained occurrences with canonical S3/CAS values.

## Evidence from PR #4343

PR #4343 demonstrates the intended late-materialization pipeline: DataFusion parses and plans SQL, a resident provider pushes safe metadata predicates, projected value IDs hydrate lazily, residual value predicates run in DataFusion, and the final limit applies after hydration. Its cloud handoff explicitly anticipates ClickHouse plus S3.

The prototype validates semantics rather than cloud scale. Its current provider is SQLite-specific and pushes only simple filters; a hosted implementation must additionally push whole safe resident joins/aggregates/order/limits into ClickHouse, or it will degrade into a large Arrow row pump. The shared conformance corpus must compare local execution with DataFusion-over-ClickHouse and also compare pushdown enabled versus disabled.
