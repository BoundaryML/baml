# ClickHouse

**Status:** Target v1 analytical contract. No Project Studio ClickHouse migrations exist on this branch. Exact physical DDL is deliberately not fabricated here.

## Role

ClickHouse is the hosted resident analytical engine behind DataFusion/BAML. It stores rebuildable non-value facts and performs safe filters, joins, grouping, ordering, rollups and limits.

It is not:

- the public parser/planner;
- canonical evidence;
- a value/body store;
- a raw tenant-accessible database; or
- the authority for run outcome, acceptance, ownership, deletion or policy.

## Absolute value-content boundary

ClickHouse must not persist:

- argument, return, error or log bodies;
- decoded JSON or BAML values;
- bounded plaintext previews;
- scalar leaves or canonical path/value rows;
- plaintext/token/ngram/full-text/vector indexes derived from values;
- query-scoped hydrated body tables after a statement; or
- pack/chunk bytes as a private KV cache.

It may persist an authorization-gated opaque provider handle plus
occurrence/role/availability metadata needed to locate a canonical S3/CAS
value. The handle representation and any identity-based equality optimization
are deferred by X4; public semantic value equality does not expose the handle.

## Public versus physical schema

Users query versioned logical relations through DataFusion/BAML. Physical ClickHouse names are private trusted mappings.

The minimum stable logical core is:

- **runs_v1**;
- **cct_population_v1**;
- **retained_calls_v1**;
- the supporting LLM, conditional spawn, exact-window, evidence-issue,
  function, call-site, and revision relations in
  [Query system](../04-query-system.md#logical-catalog).

The provider mapping must specify exact logical types/nullability, grain, keys, availability, resident/hydrated columns, capability requirements and catalog version. That mapping and the physical migrations remain v1 freeze work.

## Required resident datasets

The names below are logical/provider roles, not guaranteed physical table names.

### Runs

Required resident columns:

~~~text
scope:
  tenant_id, project_id, environment_id, projection_generation

identity/program:
  run_id, revision_id, entry_function_id, entrypoint

lifecycle:
  started_at, ended_at, duration_ns, status
  structure_state, value_state
  integrity_state, projection_state, retention_state

population summaries:
  total_calls, total_errors

snapshot/provenance:
  projected_through
  source artifact/chunk/range
  decoder/schema version
  deterministic logical row/batch identity and hash
~~~

There are no run-level LLM/token totals and no free-form degraded/diagnostic
columns. LLM facts have one aggregate home, and typed state plus the evidence
issue relation explains degraded evidence.

### CCT population

~~~text
tenant_id, project_id, environment_id, projection_generation
run_id
node_id, parent_node_id, depth
function_id, revision_id, fqn, definition_key, local_definition_hash
calls_started
calls_succeeded, calls_errored, calls_cancelled, calls_exited
inclusive_ns, self_ns, await_ns
optional fixed duration histogram
snapshot/terminal-so-far semantics
projected_through
provenance + logical row/batch identity/hash
~~~

**node_id** is run/session-epoch scoped, never global. Cross-revision grouping
uses **definition_key**, not function_id. **local_definition_hash** is projected
from the artifact's `def_content_hash`; it covers only the function's own
compiled signature and bytecode, not referenced-definition contents.

Precomputed display paths are not resident. Parent IDs and depth preserve the
tree. The histogram is resident only if P0 includes tail/percentile questions
and overflow correctness is closed.

### Excluded: CCT time buckets

A CCT time-bucket relation is not required for v1. It grows with active
call-tree locations multiplied by elapsed buckets and has a mutable open
bucket. Complete totals live in CCT population, current updates use the bounded
private live path, and retained incident detail uses exact windows. A coarse,
retention-limited derived time series requires a measured later workflow.

### LLM population

~~~text
scope + generation
run_id, node_id
provider, model
llm_calls
token_state
nullable input_tokens, output_tokens
provider_errors, parse_errors
projected_through
provenance
~~~

This relation is provisional for the current LLM functions and changes with
Aaron's work. Dollar cost is not stored as canonical fact; join emitted usage
to an authorized price relation at query time.

### Spawn aggregates and retained instances

~~~text
spawn edge:
  scope, run_id, spawning context, child function
  spawned, completed, errored, cancelled
  running_ns, awaiting_ns
  retained_instances, instances_dropped
  provenance

spawn instance:
  scope, run_id, spawn_id, edge_id, logical_thread_id
  optional retained parent_call_id, child_call_id
  status, start/end, exact-window/dump reference
  evidence reference and state
  provenance
~~~

Both spawn datasets are conditional on concurrency diagnosis being P0.

### Retained calls

Resident fields:

~~~text
scope + generation
run/process/engine/thread/call/parent identities
node_id, definition_key
call-site reference
start/end/duration/status
retention source / exact-window identity
per-role availability and policy version
opaque provider role handles (representation deferred)
projected_through
provenance + logical row/batch identity/hash
~~~

Not resident:

~~~text
args
return
error
previews
decoded values
value paths/scalars
~~~

DataFusion supplies logical hydrated columns through ValueResolver.

### Exact-window and evidence-issue ledgers

~~~text
exact window:
  scope, run_id, window_id, session_id, source, trigger
  optional trigger_node_id, trigger_call_id
  time bounds, event_count
  evidence state and typed incomplete reasons
  logical evidence ID; private provider mapping resolves object/range
  provenance

evidence issue:
  scope, issue_id, optional run/session/evidence IDs
  source, affected kind, typed reason, count
  first_seen_at, last_seen_at, optional policy version
  provenance
~~~

### Function and revision dimensions

Function columns mirror the revision dictionary's required identity, name,
source-span, kind/origin, and decoded capture-policy fields. Call-site columns
map revision-local call-site IDs to source spans once the currently empty
dictionary section is populated. Revision columns carry source
snapshot/compiler identity, capture-policy version, identity state and
first-seen time. Every behavior-affecting compiler input belongs in the
revision identity; do not add a second opaque options hash merely to duplicate
it. Function names and source paths remain classified tenant data.

### Observation/run-detail projections

The product may persist retained active/terminal observation metadata and run-scoped threads/graph/provider/tool event metadata when emitted.

Rules:

- retained instances only; never synthesize an all-call fact table;
- no value-derived preview/body/search columns;
- running facts have explicit pending/so-far state;
- exact bodies resolve through S3/CAS;
- high-cardinality run detail is queried by bounded run scope; and
- unknown future event kinds remain preservable/reprojectable.

The older observations_terminal_v1 column inventory must be reworked before freeze because it included value-derived fields and implied one row per call.

## Duplicate and conflict safety

SQS and projector work are at-least-once. User-visible SQL must still see one semantic fact.

Required:

1. deterministic logical row ID and row hash;
2. deterministic projection batch ID and row ordinal;
3. read-back after ambiguous insert;
4. identical duplicates collapse in the serving mapping;
5. same logical ID/version with a different hash becomes explicit **conflicting** integrity state;
6. disputed fields are not resolved by “latest arrival”;
7. checkpoint advances only after required visibility verifies; and
8. no common query relies on background merge timing, a finite dedup window, FINAL, or ReplacingMergeTree latest-row behavior.

For a conflicting logical identity, the serving catalog exposes one fact with **integrity_state = 'conflicting'**, suppresses disputed columns instead of picking one, and still counts the existence of that fact in population/rollup totals. The concrete suppression carrier and physical implementation freeze with DDL.

The physical mechanism is benchmark-owned.

## Projection generations

For a major decoder/physical-schema change:

1. create generation B;
2. dual-project new commits to active A and building B;
3. replay older evidence into B from a fixed barrier;
4. validate counts, hashes, evidence states and queries;
5. atomically switch the PostgreSQL active pointer;
6. keep A receiving new commits during the rollback window;
7. retire A only after audited validation.

A query binds one generation. Public view versions are independent of physical generation names.

## Physical ordering starting point

Semantics are fixed; the actual DDL is benchmark-owned. The earlier design’s starting point remains a candidate:

~~~text
PARTITION BY month(started_at)
ORDER BY (
  tenant_id,
  project_id,
  projection_generation,
  date(started_at),
  function_family_or_kind,
  started_at,
  observation_id
)
~~~

Tenant is not the partition key. Tenant/project leading order helps scoped reads. Run-ID lookup may need a measured secondary projection. Do not adopt any of this without corpus-scale benchmarks.

## Rollups

Rollups are scheduled recomputations from verified duplicate-safe population/retained facts after a lateness watermark. They are never insert-triggered aggregates over potentially duplicated raws.

Candidate rollups:

- population status counts by definition/provider/model/release/environment;
- duration histograms/distributions;
- emitted usage totals;
- evidence-state counts.

Each carries contributing count and checksum. Value-content rollups/indexes are forbidden.

## Tenancy and grants

Even though users never connect directly, ClickHouse remains defense in depth:

- API/query coordinator selects an identity at the authorization-grant profile;
- base tables carry row policies for tenant + project + environment;
- admin/service allow-all policy is explicit;
- serving views execute as INVOKER;
- grants are column-scoped;
- identities have serving-database access only;
- no system query logs/process tables;
- no file/url/s3/remote table functions;
- no temporary-table creation for tenant roles;
- distributed/parallel-replica paths remain disabled until policy propagation is proven;
- migration-time checks enumerate every base/serving table and require policy/grant coverage.

DataFusion QueryScope is the public authorization boundary. ClickHouse policy is not replaced by it.

## Budgets

ClickHouse roles/profiles constrain resident work, while DataFusion enforces the end-to-end query-global budget. Profile limits must be CONST/MAX constrained so a SETTINGS clause cannot raise them.

Required dimensions include execution time, rows/bytes read, result rows/bytes, per-query and per-identity memory, concurrency, quotas and workload classes. Projector inserts/merges keep reserved capacity.

Exact numeric values are deferred policy/benchmark choices.

## Rebuild contract

Lose ClickHouse:

1. recreate migrations and provider mappings;
2. replay the active generation from accepted S3 evidence under PostgreSQL commitment;
3. validate logical counts/hashes/outcomes/conformance;
4. activate only after verification.

ClickHouse backup may improve RTO but cannot replace this path.

## DDL freeze checklist

- Name physical tables and serving views.
- Assign concrete ClickHouse types/nullability/codecs.
- Reconcile running/pending fields.
- Define retained-call opaque handle columns without resolving X4 accidentally.
- Remove every value-derived content column.
- Define duplicate-safe serving mechanism.
- Benchmark partitions/order/projections/rollups.
- Define row policies, grant manifests, settings profiles, quotas and workload classes.
- Build migration-time policy coverage checks.
- Publish trusted logical mappings.
- Run local/hosted and pushdown-on/off conformance.
