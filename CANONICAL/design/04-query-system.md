# Query system

**Status:** Settled v1 semantics; the backend-neutral **baml_query** core
(catalog v1, DataFusion planning, D7 value lowering, budgets, outcomes)
is built and gate-tested on this branch. The Q1 freeze resolutions —
named `args` root, subscript grammar and zero-based index, absent-path/
type-mismatch behavior, the row-level unavailable carrier, and the full
column/type catalog — are recorded in the
[implementation notes](12-implementation-notes.md) and pinned by
`crates/baml_query` golden tests. Local providers (Q2) and the hosted
system are still target work.

## Contract in one sentence

Project Studio exposes one versioned BAML/DataFusion SQL surface. DataFusion
owns public semantics, snapshots, BAML value operators/traversal and
allowlisted functions, budgets, cancellation, pushdown validation, and
outcomes; local providers read canonical local evidence, while hosted
providers push safe non-value work to ClickHouse and hydrate authorized values
from S3/CAS.

It is SQL, but it is not raw physical ClickHouse SQL.

## Why this design

SQL supplies joins, grouping, windows, subqueries, external relations, agent familiarity, saved queries, and BI interoperability. The old BQL work demonstrated valuable honesty mechanisms, but maintaining a separate language and a separate hosted JSON AST was not justified.

The retained safeguards are:

- grain-named versioned relations;
- trusted logical-to-physical mappings;
- one fixed query snapshot;
- typed availability and loss facts;
- query-global budgets and cancellation;
- semantics-checked pushdown;
- ordinary SQL operators/subscripts over virtual BAML values plus a small
  platform-owned function catalog for operations with no natural SQL form; and
- a mandatory terminal query outcome.

BQL and StudioQueryV1 are superseded product surfaces. The existing **baml q** implementation remains compatibility code until SQL reaches parity.

## Architecture

~~~text
portable SQL
-> authorize and bind QueryScope
-> DataFusion logical plan
-> split resident and hydrated work
-> execute exact resident pushdown
-> stream candidate RecordBatches
-> hydrate distinct values in bounded async batches
-> evaluate BAML residual predicates/operators
-> execute final limit/aggregate/order at the correct level
-> stream rows with backpressure
-> emit mandatory query_outcome
~~~

### baml_query ownership

The backend-neutral crate owns:

- logical catalog and view versions;
- DataFusion session/planning setup;
- capability metadata;
- table-provider/provider-factory contracts;
- ValueResolver contract;
- QueryScope and snapshot binding;
- pushdown classification;
- BAML value-expression typing, analyzer rewrites, and allowlisted function
  registration;
- query-global counters, timeouts, cancellation, memory pool and spill;
- output backpressure; and
- terminal outcome creation.

It must not invent a JSON codec, content-ID scheme, or BAML value model. It uses the canonical codec and local/S3 CAS readers.

### Provider mapping

Every trusted mapping declares:

| Property | Required |
|---|---|
| Logical relation/column name | Yes |
| Logical type and nullability | Yes |
| Grain | Yes |
| Key/identity scope | Yes |
| Availability/evidence semantics | Yes |
| Resident versus hydrated | Yes |
| Required backend capability | Yes |
| Catalog/view version | Yes |
| Physical source/name | Private and trusted |

Untrusted SQL cannot select a physical table, rewrite a mapping, or supply its own tenant scope.

## Grain contract

### Population

Every runtime call contributes to population CCT aggregates. Relations named **\*_population_vN** answer complete aggregate questions at the bound snapshot.

Example:

~~~sql
SELECT definition_key, sum(calls_errored) AS failures
FROM cct_population_v1
GROUP BY definition_key
ORDER BY failures DESC;
~~~

### Retained instances

Only calls retained by capture policy, exact windows, promotion, or another explicit retention mechanism are individually discoverable.

~~~sql
SELECT count(*) AS retained_failures
FROM retained_calls_v1
WHERE status IN ('failed', 'panicked');
~~~

This count is “retained failures,” not “all failures.” The population total lives in a population relation.

### Value grains

All three operations are required over one deduplicated model:

- occurrence grain — retained executions that reference a matching value;
- unique-root grain — distinct captured values and occurrence counts;
- scalar/path grain — matching structured leaves at canonical paths.

The exact virtual-relation names are not frozen. None of these requires persistent value content in ClickHouse.

## Logical catalog

The following is the reconciled relation inventory. A relation marked **freeze required** has settled meaning but incomplete literal columns/types in the source material. Physical tables remain private.

### Shared resident relations

| Relation | Grain | Known required columns | Status |
|---|---|---|---|
| **runs_v1** | one boundary/run | run_id, started_at, ended_at, duration_ns, status, revision_id, entry function/entrypoint, total_calls, total_errors, structural/value/integrity/projection/retention states | Required; no run-level LLM totals or free-form degraded/diagnostic columns |
| **cct_population_v1** | run × call-tree location at bound snapshot | run/node/parent/depth, function/revision/definition identity including local definition hash, started and terminal counts, inclusive/self/await time, optional fixed histogram | Required; no precomputed display path; local hash is not dependency-closure identity |
| **llm_population_v1** | run × call-tree location × provider × model | run/node/provider/model identity, llm_calls, token availability, nullable input/output tokens, provider_errors, parse_errors | Provisional pending Aaron's LLM changes |
| **spawn_edges_v1** | aggregate spawn edge | run/parent-location/child-function identity, spawned/completed/errored/cancelled counts, running/awaiting totals, retained-instance and dropped counts | Conditional on concurrency diagnosis being P0 |
| **spawn_instances_v1** | retained spawn instance | run/edge/spawn/thread identity, optional retained parent/child calls, status, start/end, exact-window/evidence references and state | Conditional retained-instance grain |
| **exact_windows_v1** | one retained exact-evidence region | run/window/session identity, source/trigger, optional trigger node/call, time bounds, event count, evidence state/reasons and logical evidence ID | Required evidence ledger; event bodies stay outside ClickHouse |
| **evidence_issues_v1** | one immutable grouped issue summary | issue/run/session/evidence identity, source, affected kind, typed reason, count, first/last seen, optional policy version | Required; not one row per affected event |
| **functions_v1** | revision × function | revision/function/definition identity, local definition hash, names, source span, kind/origin, decoded capture policy | Required; local hash covers the function's own compiled definition only |
| **call_sites_v1** | revision × call site | revision/call-site identity and source path/span/line | Target metadata required with retained-call source navigation; current dictionary emits no rows |
| **revisions_v1** | one compiled revision | revision/source-snapshot/compiler identity, capture-policy version, identity state, first seen | Required; revision identity must commit to every behavior-affecting compiler input |

**cct_windows_v1 is not in the minimal v1 catalog.** It grows with active
call-tree locations multiplied by elapsed time buckets and has mutable open
buckets. Complete totals come from **cct_population_v1**, current updates use
the bounded private live path, and retained incident detail is represented by
**exact_windows_v1**. Add a coarse, retention-limited derived time series only
after a measured historical-chart workflow justifies it.

### Retained-call relation

**retained_calls_v1** is the primary retained-instance relation named by the latest decision log.

Its contract has three deliberately separate layers.

Query-visible resident fields:

| Column family | Required fields |
|---|---|
| Identity | run_id, thread_id, call_id, parent_call_id, aggregate node reference |
| Function | definition key and call-site reference; other function metadata joins through the run revision and function dictionary |
| Lifecycle | start/end, monotonic duration, status/execution state |
| Retention | retention reasons and exact-window/evidence references |
| Availability | per-role pending/available/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported and policy version |

Provider-private resident fields:

| Column family | Required fields |
|---|---|
| Scope | tenant/project/environment, projection generation |
| Value lookup | authorization-gated opaque role handles; never value bodies, CIDs, S3 keys, or byte ranges in the public schema |
| Provenance | source artifact/range/record, durable watermark, deterministic row/batch identity and hash |

Virtual query fields:

| Field | Source |
|---|---|
| **args** | Captured input value resolved from authorized local evidence or S3/CAS after resident filtering |
| **return** | Captured output value resolved from authorized local evidence or S3/CAS after resident filtering |
| **error** | Captured error value resolved from authorized local evidence or S3/CAS after resident filtering |

The virtual **args/return/error** fields are part of public SQL, not physical
ClickHouse columns. The query engine supplies them only when a statement needs
them. Customer content never lives in ClickHouse.

The exact Arrow types, nullability, role-column spelling, and carrier for a predicate that is unknown because a value is unavailable must be frozen before v1 DDL/API publication. The normative rule is already fixed: unavailability cannot become an ordinary NULL or silent non-match, and the terminal outcome must reconcile it.

### Hosted observation relations

The Studio product also needs:

- **observations_active_v1** — retained running/incomplete operations;
- **observations_terminal_v1** — retained terminal operations;
- run-detail calls/threads/graph/event relations;
- evidence-state relations; and
- projection visibility/integrity relations.

Older designs modeled an apparent row for every call and included value previews. That shape is not canonical. Before freeze:

1. re-grain observation/call relations to retained instances only;
2. remove body, preview, scalar/path and text-search content from ClickHouse;
3. include durable running state and pending semantics; and
4. add the required row-level availability/provenance fields.

### Virtual hydrated value operations

DataFusion must support scoped occurrence/root/path operations and typed path
predicates. A provider may expose a query-scoped hydrated relation, but no
standing **value_nodes_v1**, **value_scalars_v1**, or **value_bodies_v1** table
is persisted in ClickHouse.

The public surface uses ordinary SQL expressions over the virtual BAML `value`
type:

~~~sql
-- Exact whole-value equality against an observed canonical value.
WHERE args = baml_value_cid('bamlv_1_…')

-- Nested scalar comparison (args is a named-argument object).
WHERE args['customer']['age'] >= 30

-- Returned-field comparison.
WHERE "return"['status'] = 'rejected'
~~~

The equality operator means canonical BAML semantic equality. It never means
partial-object matching, serialized-byte equality, public CID equality, or
opaque-handle equality. A provider may optimize equality using identity only
when it proves that the optimization is equivalent to the public semantics.

Frozen (IN-Q1-1/2): `args` is a named-argument object keyed by declared
parameter name — a numeric subscript on the `args` root is a planning
error with a remedy; string subscripts select object/class/map fields and
integer subscripts select list elements, zero-based. DataFusion
recognizes the virtual value type and lowers these expressions into
internal hydration, traversal, runtime type checking, and comparison
expressions. Internal UDF names are not a public or versioned contract.
The platform-owned value functions are `baml_value_cid('bamlv_1_…')` and
`baml_value_json('{…}')`, valid only as equality operands against value
fields.

An unavailable value produces the D12 typed-unknown evaluation and contributes
to the terminal outcome; it is not SQL `NULL` or false. A captured BAML null is
ordinary null-like data. Frozen (IN-Q1-3): over an AVAILABLE value, an
absent path or an incompatible leaf comparison is an ordinary SQL-NULL-
like non-match and the result stays complete — deliberately distinct
from typed unavailability, which always reconciles in the outcome.

## Value-query execution

Consider:

~~~sql
SELECT call_id
FROM retained_calls_v1
WHERE args['customer']['age'] >= 30
LIMIT 100;
~~~

The execution contract is not:

~~~text
ClickHouse LIMIT 100 -> hydrate those 100 -> hope they match
~~~

It is:

~~~text
resident project/time/function filters
-> stream candidate rows and authorized handles
-> deduplicate handles per bounded cache
-> batch/range-read canonical values
-> budgeted recursive decode
-> typed path/value predicate
-> continue until 100 actual matches or end/budget/cancel
-> outcome
~~~

A final limit can be pushed only when every predicate before it is exactly executed by the resident provider. An **InexactCandidate** predicate is always rechecked by DataFusion.

Candidate-set size alone does not turn an ordinary query into a deferred job. Large finite scans stream. A future durable background operation is separate and deferred.

## Query snapshot

At start, every ordinary query binds:

- public catalog/view version;
- projection generation;
- durable projected-through/evidence barrier;
- tenant/project/environment scope;
- provider-specific snapshot handles; and
- authorization state.

Every resident provider and every value read uses that snapshot. Later commits are invisible. Authorization revocation is stronger than snapshot stability: an erasure/tombstone denies access and cancels affected active queries.

Running executions at the watermark are included. Not-yet-emitted return/error/end facts are **pending**, and counters are explicitly “so far.”

## Availability and outcomes

### Data-level failure

Missing, corrupt, unsupported, redacted, not-captured, truncated, or otherwise unavailable values:

- do not fail an unrelated cohort query;
- do not become an ordinary NULL/non-match;
- retain a typed reason in row/predicate evaluation state; and
- make the result incomplete unless the query did not require that value.

### Query-wide failure

| Condition | Result |
|---|---|
| Insufficient value-read permission | Fail before execution |
| S3/CAS dependency unavailable | Retryable query failure |
| Budget exhausted | **E_QUERY_BUDGET_EXCEEDED**, rows already streamed explicitly incomplete |
| Cancelled | Typed cancellation |
| Backend-only function used locally | **E_BACKEND_CAPABILITY** during planning |
| Stream ends without outcome | Caller may not claim successful completion |

### Terminal outcome

Every SQL stream ends with exactly one out-of-band terminal outcome, including success, evidence-incomplete success, planning/execution failure, budget exhaustion, and cancellation. A failure before rows yields an outcome with no data batches; a failure after rows marks those rows incomplete. A transport truncation that prevents receipt of the outcome is itself distinguishable and never counts as successful completion.

The example below is illustrative. Exact field names, enum values, framing, and the row-level typed-unknown carrier freeze with catalog v1:

~~~json
{
  "queryCompleted": true,
  "resultState": "complete",
  "snapshot": {
    "catalogVersion": "v1",
    "projectionGeneration": 7,
    "projectedThrough": "..."
  },
  "valueEvaluations": {
    "attempted": 12,
    "available": 12,
    "unavailable": 0,
    "byReason": {}
  }
}
~~~

Human rows go to stdout and a compact outcome to stderr. Structured streaming uses a terminal control frame. It is not a synthetic SQL row and not a second query language.

## Function catalog

V1 permits:

1. the documented portable SQL/DataFusion functions;
2. platform-owned BAML value navigation/conversion/predicate functions; and
3. an explicit allowlist of ClickHouse-backed extensions whose syntax fits the same grammar.

V1 forbids:

- CREATE FUNCTION;
- user BAML functions registered as SQL UDFs;
- arbitrary uploaded code/plugins;
- a second ClickHouse parser or raw dialect passthrough;
- physical table access; and
- silent local-to-hosted routing.

Example backend failure:

~~~text
E_BACKEND_CAPABILITY
function: clickhouse.quantileExact
required_backend: clickhouse
current_backend: local
~~~

The function is rejected before any data read.

## Budgets and streaming

Exact numeric values are policy-deferred, but the implementation must count globally:

- candidate rows;
- distinct value handles/CIDs;
- object requests and downloaded bytes;
- decoded bytes, depth, nodes, array elements and string bytes;
- CPU/wall time;
- memory and spill;
- result rows/bytes;
- concurrency; and
- tenant fairness.

Counters never reset per input batch. Execution streams RecordBatches rather than calling collect for the full result. Hydration caches are bounded. Cancellation covers resident scans, value reads, decoding, residual work, spill, and output.

Global sorts, windows, high-cardinality grouping and joins may require bounded state/spill even when hydration is streaming.

## Local provider contract

Local canonical sources are **.baml** artifacts and local CAS packs. Provider choice is per relation:

- SQLite for small/update-friendly resident catalogs;
- Parquet for columnar scan relations;
- direct artifact/fold providers for already efficient native formats.

The v1 benchmark chooses. No provider may change public semantics.

**control.sqlite** is not the analytical provider. It remains non-rebuildable control state.

## Hosted provider contract

ClickHouse owns non-value resident facts and can execute filters, joins, grouping, ordering and safe limits when translation is proven. DataFusion remains the public planner and normative residual executor.

The hosted provider must avoid becoming an Arrow row pump. It should push the largest safe resident subplan, then stream only the candidate identities/handles needed for residual work.

ClickHouse cannot persist:

- arguments, returns, errors, or log bodies;
- decoded JSON/BAML values;
- previews;
- scalar/path rows;
- plaintext/token/ngram indexes over customer values; or
- a private chunk-KV copy of customer bodies.

## Cross-plane questions

V1 has no local-plus-hosted federation planner. Supported product shapes are:

- upload/promote a local run, then query it hosted;
- run the same portable statement separately with local/hosted routing and label the results; or
- export bounded hosted data and join it locally through an explicit external relation if the local provider supports it.

Hosted handle representation and any identity-based equality optimization are
deferred by X4. Public whole-value semantic equality is fixed by D7. No v1
example may promise that a raw local CID joins directly to a hosted handle.

## Conformance requirements

The corpus must compare:

- local providers against hosted DataFusion-over-ClickHouse;
- pushdown enabled versus disabled;
- population versus retained-instance trap cases;
- fixed snapshots while ingest continues;
- running/pending semantics;
- missing/redacted/corrupt values and reconciled outcomes;
- natural whole-value equality and nested subscript predicates against a
  canonical BAML-value reference evaluator;
- captured null, unavailable evidence, absent path, and incompatible leaf type
  remain distinguishable under the Q1-frozen rules;
- final limit after hydrated predicates;
- cancellation across all stages;
- query-global budgets across many batches;
- cross-revision identity;
- NaN and positive/negative zero behavior from the canonical codec;
- duplicate physical analytical writes; and
- backend capability failures before reads.

An agent evaluation runs against schema documentation alone before catalog v1 freezes.

## Prototype evidence

[PR #4343](https://github.com/BoundaryML/baml/pull/4343) is an open, unmerged local DataFusion/SQLite prototype. It demonstrates:

- DataFusion planning;
- resident filter pushdown;
- lazy value hydration;
- residual value predicates;
- final-limit placement;
- logical/physical column mappings;
- cancellation/budgets; and
- a future ClickHouse/S3 handoff shape.

It does not define the production physical model. Specifically rejected:

- loose SHA-256 JSON blobs;
- serialized `LargeBinary` JSON plus required public helper chains as the BAML
  value-expression contract;
- an apparently population-true all-call **function_calls** table;
- NULL-only availability;
- per-batch limits presented as query-global; and
- SQLite as an architectural invariant.

## Freeze checklist

Done in Q1 (each pinned by `crates/baml_query` golden tests and recorded
in the [implementation notes](12-implementation-notes.md)):

- every v1 relation and column enumerated with Arrow types, nullability,
  keys, and identity scopes (`catalog::catalog_v1`, column golden test);
- per-role availability columns (`args_state`/`return_state`/
  `error_state`) plus the frozen unavailable carrier: an undecidable row
  leaves the data stream and reconciles in `query_outcome` — ordinary
  row schemas never change;
- the `args` root shape, subscript grammar/index base, absent-path/
  type-mismatch behavior, and the platform value functions
  (`baml_value_cid`, `baml_value_json`);
- the terminal outcome wire schema (`outcome::QueryOutcome`, camelCase
  serialization) and saved-query guidance (versioned names portable;
  unversioned aliases pinned to the session's bound catalog version).

Remaining for Q2/H2: publish local/hosted provider mappings, the
capability matrices as providers land, and the cross-backend conformance
fixture corpus.
