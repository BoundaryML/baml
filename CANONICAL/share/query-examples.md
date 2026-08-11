# Project Studio data model and query examples

> **Start with the story instead.** This document is now the appendix-grade
> schema reference. The eased-in, reviewer-facing version — which teaches the
> vocabulary before using it — is
> [share/story/00-start-here.md](story/00-start-here.md).

## Start with four questions

1. What happened across every call?
2. Which exact call should I inspect?
3. What evidence is missing or unavailable?
4. Which source revision produced the behavior?

One row per function invocation would make ingest and storage grow directly
with application traffic. Instead, Project Studio keeps complete small
summaries for every call and retains individual calls only when policy selects
them.

## Where data lives

| Place | Contents |
| --- | --- |
| Local `.baml` files | Authoritative local run evidence and captured values |
| S3 | Authoritative uploaded evidence and value/log bodies |
| ClickHouse | Rebuildable small facts for filters, joins, counts, grouping, and ordering |
| PostgreSQL | Ownership, upload, projection, audit, retention, and deletion workflow |
| Public SQL | Resident fields plus explicitly marked virtual fields, independent of physical table names |

The proposed schemas below are **logical public relations**, not promises of
one physical ClickHouse table per relation. Hosted relations combine immutable
ClickHouse facts with small control-plane or active-state overlays where noted.
Provider-private scope, projection, provenance, relationship-link, and
storage-handle columns are omitted. Virtual fields are listed separately and
never imply a ClickHouse column.

## How this maps to columnar, immutable storage

ClickHouse stores and compresses physical columns independently. These
relations therefore favor typed scalar columns used for filtering, grouping,
and ordering; large values and detailed event bodies remain in local evidence
or S3. Repeating stable function identity in the hottest aggregate relation is
deliberate: it compresses well and avoids a dimension join on common queries.

ClickHouse is the rebuildable analytical projection. Normal ingestion writes
deterministic immutable facts in batches; it does not repeatedly update one
ClickHouse row as a run advances. The public relation resolves those facts at
the query's fixed watermark.

| Logical row kind | Physical pattern |
| --- | --- |
| Compiled metadata: `functions`, `call_sites`, `revisions` | Insert once per revision; immutable |
| Sealed evidence: terminal `runs`, `retained_calls`, `spawn_instances`, `exact_windows`, `evidence_issues` | Insert only after the source scope is sealed; immutable |
| Running state | Small active overlay or immutable state snapshots; never an in-place rewrite of the terminal fact |
| Population totals: `cct_population`, `llm_population`, `spawn_edges` | Immutable batched deltas while active, then one immutable final aggregate per logical key |
| Growing relationships such as call-to-window or call-to-evidence | Append-only link facts; bounded public lists are assembled by the query provider |

For an aggregate that has both active deltas and a final row, the provider uses
the final row when it is visible at the bound watermark; otherwise it sums the
verified deltas. It never adds both. Private active/delta facts may be compacted
or expired after finalization; they are not an indefinite public time series.

Corrections and integrity conflicts append explicit issue/tombstone facts or
build a new projection generation. They do not use latest-arrival-wins.
Authorized erasure is an exceptional controlled delete/rewrite path, not the
normal ingestion mechanism. Common queries must not depend on mutation
completion, background deduplication, `FINAL`, or `ReplacingMergeTree` merge
timing. This follows ClickHouse's
[immutable-data guidance](https://clickhouse.com/docs/concepts/best-practices/avoid-mutations).

## Schema rules

Logical types used below:

- `id` — opaque stable identifier;
- `timestamp` — UTC instant;
- `count` — non-negative 64-bit counter;
- `duration_ns` — non-negative nanosecond duration;
- `enum` — documented closed set;
- `value` — virtual BAML value loaded from local evidence or S3 on demand;
- `list<T>` — bounded logical list of `T`; incrementally discovered membership
  is backed by immutable link facts rather than array updates; and
- `?` — logically nullable; it does not require a physical ClickHouse
  `Nullable` when an explicit state plus a non-null value column is clearer.

Physical tables still need tenant/project scope, projection generation, row
hashes, source ranges, and opaque value handles. Those are provider machinery,
not public query columns. Query snapshot and projection-watermark information
belongs in `query_outcome` rather than every row.

Every key named below is a **logical uniqueness key**. ClickHouse
[primary keys are sparse ordering indexes](https://clickhouse.com/docs/concepts/best-practices/choosing-a-primary-key),
not uniqueness constraints; physical `ORDER BY`, partitioning, codecs, and
secondary projections freeze from measured query workloads rather than from
these logical keys.

## 1. `runs`

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key | Gives the run a stable identity and joins every run-scoped relation. |
| `started_at` | `timestamp` | Supports time-range filters and chronological run lists. |
| `ended_at` | `timestamp?`; absent while running | Distinguishes open from closed runs and records the wall-clock interval. |
| `duration_ns` | `duration_ns`; exact elapsed or so-far time | Preserves monotonic elapsed time; subtracting wall-clock timestamps is not reliably exact. |
| `status` | `enum`; pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned | Answers where the run is in its execution lifecycle without inferring from timestamps or errors. |
| `revision_id` | `id`; joins `revisions` | Connects observed behavior to the exact compiled program and its function dictionary. |
| `entry_function_id` | `id?` | Provides a stable root-function join when the entrypoint is a BAML function. |
| `entrypoint` | `string` | Gives users a readable command, test, or function name, including entrypoints that are not functions. |
| `total_calls` | `count` | Makes run-list call volume cheap to read without scanning call-tree summaries. |
| `total_errors` | `count` | Makes runs with any errored calls cheap to find without scanning call-tree summaries. |
| `structure_state` | `enum`; complete/incomplete/pending/lost | Says whether the call structure is complete; execution success does not prove structural evidence is complete. |
| `value_state` | `enum`; complete/partial/pending/not_captured/lost | Says whether arguments, returns, and errors can be inspected; missing values must not look like ordinary SQL `NULL`. |
| `integrity_state` | `enum`; verified/unverified/corrupt/conflicting | Separates trustworthy evidence from evidence that arrived corrupt or disagreed across sources. |
| `projection_state` | `enum`; pending/active/delayed/failed/rebuilding | Tells users whether ClickHouse is current; a healthy run can still be absent from or delayed in the projection. |
| `retention_state` | `enum`; retained/partially_retained/erased | Explains whether previously captured evidence remains available after retention or deletion. |

### Why it exists

- **Row:** one program run.
- **Growth:** one small row per run, not per call.
- **Keep:** yes; every workflow starts by finding a run.
- **Enables:** run lists, lifecycle/revision filters, error totals, entrypoint
  display, and evidence-health filters.

`total_calls` and `total_errors` are derivable from call-tree summaries, but
keeping them avoids scanning another table for each run-list page.
`entry_function_id` is the stable join; `entrypoint` also handles tests and
commands that are not functions.

The terminal ClickHouse fact is immutable. Before terminal evidence exists,
`status`, so-far duration/totals, and pending evidence states come from the
bound active-state overlay. `projection_state` and `retention_state` come from
PostgreSQL control state because a failed or erased ClickHouse projection
cannot authoritatively describe itself. The public `runs` relation composes
those sources without rewriting the terminal row.

### Which recent runs had problems?

**English:** List recent failed, panicked, or abandoned runs, plus runs with
errors, incomplete structure, partial/lost value evidence, failed integrity
verification, or delayed/failed projection.

```sql
SELECT
    run_id,
    started_at,
    ended_at,
    duration_ns,
    status,
    revision_id,
    entrypoint,
    total_calls,
    total_errors,
    structure_state,
    value_state,
    integrity_state,
    projection_state,
    retention_state
FROM runs
WHERE started_at >= :from_time
  AND (
    total_errors > 0
    OR status IN ('failed', 'panicked', 'abandoned')
    OR structure_state <> 'complete'
    OR value_state IN ('partial', 'lost')
    OR integrity_state <> 'verified'
    OR projection_state IN ('delayed', 'failed')
  )
ORDER BY started_at DESC
LIMIT 100;
```

This query reads no arguments, responses, error bodies, or S3 objects.

## 2. `cct_population`

The historical name means **call-tree summaries**.

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `node_id` | Scopes the call-tree location to one run and joins run lifecycle and revision data. |
| `node_id` | `id`; call-tree location within the run | Gives each distinct call path a stable identity for parent links and retained-call joins. |
| `parent_node_id` | `id?`; absent for the root | Reconstructs the call tree and attributes nested work to its caller. |
| `depth` | `integer` | Makes indentation, depth filters, and bounded tree views cheap without recursive traversal. |
| `function_id` | `id`; identity within one revision | Joins the location to its compiled function metadata. |
| `revision_id` | `id`; repeated for fast grouping | Allows hot cross-run and cross-revision grouping without joining through `runs`, which is the reason to accept this duplication. |
| `definition_key` | `string?`; absent for synthetic functions | Groups the same logical function across revisions without a dimension join. |
| `local_definition_hash` | `bytes?` | Separates observations whose function's own compiled signature or body changed; it does not include the contents of referenced types or functions. |
| `fqn` | `string`; full function name | Supports common display and name grouping without a dimension join. |
| `calls_started` | `count` | Provides total demand and is the base denominator for rates and average time. |
| `calls_succeeded` | `count` | Separates successful terminal calls from failures, cancellation, and other exits. |
| `calls_errored` | `count` | Finds failing locations and computes error rates over all calls. |
| `calls_cancelled` | `count` | Keeps cancellation distinct from application failure. |
| `calls_exited` | `count`; other explicit terminal exits | Accounts for terminal outcomes not represented by success, error, or cancellation, so outstanding work is not overstated. |
| `inclusive_ns` | `duration_ns`; function plus nested calls | Finds call paths responsible for end-to-end run time. |
| `self_ns` | `duration_ns`; direct execution only | Finds functions doing work themselves rather than merely containing slow descendants. |
| `await_ns` | `duration_ns`; suspended/waiting time | Separates waiting from active execution when diagnosing latency. |
| `duration_histogram` | `list<count>`; fixed catalog-owned buckets | Enables approximate tail and percentile questions. Remove it if those questions are not P0, because totals and means do not require it. |

### Why it exists

- **Row:** one distinct call-tree location within one run.
- **Growth:** unique call paths, not repeated invocations.
- **Keep:** yes; this provides complete all-call analysis without one row per
  invocation.
- **Enables:** complete call/error counts, inclusive/direct/waiting time, and
  duration distributions.

One million calls from the same parent to the same function contribute to one
logical aggregate key. They do not cause one million ClickHouse updates: the
profiler folds them in memory, the projector writes immutable batched deltas,
and run finalization writes one immutable final aggregate. A different parent
creates another logical key. Highly dynamic paths can still grow the table, so
path-count and memory tests remain release gates.

Function identity is duplicated from `functions` deliberately: these are the
hottest aggregate queries, and avoiding a dimension join is worth the small
repetition. Display paths are not stored.

The histogram is necessary only if tail/percentile analysis is P0. If kept,
the current folded-counter overflow gap must be fixed before calling it exact.

### Which functions fail most often?

**English:** Across every finished call in a time range, rank functions by
errors and failure rate. Started-but-still-running calls are shown separately
and are not included in the denominator.

```sql
SELECT
    definition_key,
    fqn AS function_name,
    SUM(calls_started) AS calls_started,
    SUM(calls_succeeded + calls_errored + calls_cancelled + calls_exited)
        AS calls_finished,
    SUM(calls_errored) AS failures,
    1.0 * SUM(calls_errored) / NULLIF(
        SUM(calls_succeeded + calls_errored + calls_cancelled + calls_exited),
        0
    ) AS failure_rate
FROM cct_population
WHERE run_id IN (
    SELECT run_id
    FROM runs
    WHERE started_at >= :from_time
      AND started_at < :to_time
)
  AND definition_key IS NOT NULL
GROUP BY definition_key, fqn
HAVING SUM(calls_errored) > 0
ORDER BY failures DESC
LIMIT 50;
```

### Where did one run spend its time?

**English:** Rank functions in one run by direct execution time and show
inclusive and waiting time.

```sql
SELECT
    definition_key,
    fqn AS function_name,
    SUM(calls_started) AS calls,
    SUM(inclusive_ns) AS inclusive_ns,
    SUM(self_ns) AS self_ns,
    SUM(await_ns) AS await_ns,
    SUM(self_ns) / NULLIF(SUM(calls_started), 0) AS mean_self_ns_per_entry
FROM cct_population
WHERE run_id = :run_id
GROUP BY definition_key, fqn
ORDER BY self_ns DESC
LIMIT 50;
```

The mean is directional, not a percentile or proof of a regression.

## 3. `retained_calls`

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `call_id` | Scopes the retained invocation to one run and joins run/revision context. |
| `call_id` | `id` | Gives the exact invocation a stable identity for lookup and causal links. |
| `parent_call_id` | `id?`; parent may not be retained | Preserves exact parentage when known without requiring the parent itself to have been retained. |
| `node_id` | `id`; joins `cct_population` | Connects the retained example to the complete call-tree summary that led the user to inspect it. |
| `process_id` | `id`; conditional | Prevents identity collisions only if process scope is not already encoded by `run_id` and `call_id`; otherwise remove it. |
| `engine_id` | `id`; conditional | Prevents identity collisions only if engine scope is not already encoded by the other IDs; otherwise remove it. |
| `thread_id` | `id`; logical execution thread | Supports per-thread ordering and concurrency diagnosis for retained calls. |
| `definition_key` | `string?`; duplicated for common filters | Lets value and exact-call queries narrow by logical function before loading evidence. |
| `call_site_id` | `id?`; joins `call_sites` through the run revision | Navigates an invocation to the source expression that made the call. |
| `started_at` | `timestamp` | Orders retained calls and places them on the run timeline. |
| `ended_at` | `timestamp?`; absent while running | Indicates whether the invocation is still open and bounds its wall-clock interval. |
| `duration_ns` | `duration_ns`; exact monotonic or so-far duration | Supplies exact elapsed time even when wall clocks shift or the call is still running. |
| `status` | `enum`; pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned | Selects exact examples by lifecycle outcome without inferring state from optional values. |
| `retention_reasons` | `list<enum>`; policy/incident/promotion/explicit | Explains why this call exists in a selective table; a call may have more than one reason. |
| `exact_window_ids` | `list<id>` | Links the call to every retained incident window that contains it. |
| `evidence_ids` | `list<id>` | Identifies authoritative evidence that contributed to the call; these are joinable logical IDs, not S3 keys, CIDs, or byte ranges. |
| `capture_policy_version` | `integer` | Explains which capture rules decided whether values should exist. |
| `args_state` | `enum`; available/pending/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported | Distinguishes a real null argument from every reason arguments cannot be returned. |
| `return_state` | `enum`; available/pending/not_applicable/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported | Distinguishes a real null return from no return, capture policy, loss, or corruption. |
| `error_state` | `enum`; available/pending/not_applicable/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported | Distinguishes a real null error payload from a successful call or unavailable evidence. |

### Virtual query fields — not ClickHouse columns

These fields exist in the public SQL relation only. After resident filters
narrow the candidate calls, the query engine follows the call's private
evidence handles and resolves the requested values from local evidence or S3.

| Field | Type | Why expose it in SQL |
| --- | --- | --- |
| `args` | `value` or typed unavailable; resolved on demand | Lets users inspect captured inputs and apply bounded value predicates without exposing storage layout. |
| `return` | `value` or typed unavailable; resolved on demand | Lets users inspect captured outputs and apply bounded value predicates without copying output bodies into ClickHouse. |
| `error` | `value` or typed unavailable; resolved on demand | Lets users inspect captured error detail without copying error bodies into ClickHouse. |

### Public value syntax

Users write ordinary operators and subscripts against virtual values:

```sql
-- Exact equality against one complete canonical value.
WHERE args = baml_value_cid('bamlv_1_…')

-- A predicate over a nested scalar (args is a named-argument object).
WHERE args['customer']['age'] >= 30

-- A predicate over a returned field.
WHERE "return"['status'] = 'rejected'
```

Whole-value `=` means canonical semantic equality, not partial-object
matching, serialized-byte equality, or storage-ID equality. The frozen
operands are `baml_value_cid('bamlv_1_…')` (an observed canonical value,
usually copied from a prior result's `cid`) and `baml_value_json('{…}')`
(a JSON-built map/list/scalar; JSON cannot express classes/enums/media —
compare those by CID or nested scalars).

The Q1 freeze (IN-Q1-1/2/3): **`args` is a named-argument object keyed by
declared parameter name** — the captured artifact stores arguments by
name and the canonical codec orders map keys by bytes, so argument
position is not part of the canonical value; a numeric subscript on the
`args` root is a planning error with a remedy. String subscripts select
object/class/map fields; integer subscripts select list elements,
zero-based. Over an available value, an absent path or an incompatible
leaf comparison is an ordinary SQL-NULL-like non-match (the result stays
complete); unavailable evidence is a typed unknown reconciled in
`query_outcome`, never a silent non-match.

DataFusion recognizes expressions over the virtual BAML `value` type and
lowers them to internal hydration, traversal, type checking, and comparison
operations. Those internal functions are not part of public SQL. A captured
BAML null is SQL null-like data; pending, redacted, lost, corrupt, or otherwise
unavailable evidence is a typed unknown recorded in `query_outcome`, never an
ordinary `NULL` or silent non-match.

### Why it exists

- **Row:** one individually retained call.
- **Growth:** retained calls, bounded by capture and retention policy.
- **Keep:** yes, but never imply that it contains all calls.
- **Enables:** exact-call lists, causal links, source navigation, targeted value
  reads, and value predicates over a narrowed cohort.

`node_id` links the exact call to the complete summary that led the user to it.
`definition_key` is the only duplicated function field because filtering exact
calls by logical function is common. Names and hashes remain available through
the run/node joins.

`duration_ns` stays despite start/end timestamps because it uses a monotonic
clock. Q1 should remove `process_id` or `engine_id` if `run_id` already prevents
identity collisions. The physical provider also needs private opaque handles
for each captured role; `evidence_ids` is the public evidence link, not the S3
object key or byte range.

The terminal call fact is immutable. A retained call still executing appears
through the active-state overlay; terminal status, duration, and value states
are inserted only when their source scope seals. `retention_reasons`,
`exact_window_ids`, and `evidence_ids` are bounded public lists assembled from
append-only provider link facts, so discovering another containing window does
not rewrite the call row.

The three value-state columns are required: unavailable must not silently mean
ordinary SQL `NULL` or predicate non-match. The exact “could not evaluate”
carrier remains Q1 freeze work.

### Which retained failures should I inspect?

**English:** Within one run, list the slowest failed or panicked calls for which
individual evidence was retained.

```sql
SELECT
    call_id,
    run_id,
    definition_key,
    duration_ns,
    status
FROM retained_calls
WHERE run_id = :run_id
  AND status IN ('failed', 'panicked')
ORDER BY duration_ns DESC
LIMIT 100;
```

This is retained failures, not every failure. Use `cct_population` for the
complete count.

### What did one retained call receive and produce?

**English:** Load the captured arguments, return value, and error for one call.

```sql
SELECT
    call_id,
    args,
    "return",
    error
FROM retained_calls
WHERE run_id = :run_id
  AND call_id = :call_id;
```

The IDs narrow the candidate set before local/S3 value loading. `args`,
`return`, and `error` are virtual query fields in this statement, not resident
ClickHouse columns.

### Which retained calls had exactly these arguments?

**English:** Within one run and function, find retained calls whose complete
captured argument value exactly equals an observed canonical value.

```sql
SELECT
    call_id,
    run_id,
    definition_key
FROM retained_calls
WHERE run_id = :run_id
  AND definition_key = :definition_key
  AND args = baml_value_cid('bamlv_1_…')
LIMIT 100;
```

This is whole-value equality. It does not mean “contains these fields.”

### Which retained calls contain a particular value?

**English:** Within one run and function, find up to 100 retained calls whose
first argument contains a customer age of at least 30.

```sql
SELECT
    call_id,
    run_id,
    definition_key
FROM retained_calls
WHERE run_id = :run_id
  AND definition_key = :definition_key
  AND args['customer']['age'] >= 30
LIMIT 100;
```

Cheap run/function filters execute first. Values load in bounded, deduplicated
batches through the virtual `args` field. DataFusion evaluates the nested
comparison, and the limit applies only after that value condition.

## 4. `evidence_issues`

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `issue_id` | `id`; logical key for one sealed summary | Gives an immutable grouped issue a stable identity for deduplication and audit. |
| `run_id` | `id?`; absent before run binding | Attributes the issue to a user-visible run when that association is known. |
| `session_id` | `id?`; absent for non-runtime issues | Scopes runtime evidence that exists before or outside a single run association. |
| `evidence_id` | `id?`; sealed evidence range summarized | Identifies the retained evidence whose completeness or integrity is affected; provider-private metadata resolves its storage location. |
| `source` | `enum`; profiler/value_capture/uploader/projector/retention | Identifies which stage reported the problem, which determines ownership and remediation. |
| `kind` | `enum`; evidence affected | Says whether structure, values, detailed events, or another evidence class is incomplete. |
| `reason` | `enum`; typed cause | Makes causes groupable and queryable without parsing free-form messages. |
| `count` | `count`; affected facts | Compresses repeated identical problems so the table does not grow one row per failed event. |
| `first_seen_at` | `timestamp` | Marks when the grouped issue began and supports incident ordering. |
| `last_seen_at` | `timestamp` | Marks the observed extent of the grouped issue and whether it persisted. |
| `policy_version` | `integer?` | Explains policy-caused omissions; it may be absent for integrity or infrastructure failures unrelated to capture policy. |

### Why it exists

- **Row:** one immutable source-scope and kind/reason summary.
- **Growth:** only scopes containing an issue; repeated identical issues are
  counted before insertion.
- **Keep:** yes; correctness requires an explicit account of missing evidence.
- **Enables:** evidence-quality summaries and defensible completeness claims.

If each affected call becomes a row, this table recreates traffic-proportional
growth. The grouped-row contract must freeze in Q1. Current capture-loss
records are one input; integrity, projection, and retention diagnostics use the
same typed issue shape instead of a free-form run message.

An issue row is emitted only when its source range is sealed, so `count`,
`first_seen_at`, and `last_seen_at` never increment in place. If a run binding
is discovered later, an immutable provider link associates the issue with the
run; the sealed summary is not rewritten.

### Is the evidence complete enough to trust?

**English:** Summarize why evidence is missing or degraded for one run.

```sql
SELECT
    kind,
    reason,
    SUM(count) AS affected_records
FROM evidence_issues
WHERE run_id = :run_id
GROUP BY kind, reason
ORDER BY affected_records DESC;
```

Some profiler paths still fail to persist diagnostics consistently; C1 closes
that gap.

Every query also sends `query_outcome`. It identifies the data snapshot,
unavailable required values, budget exhaustion, and cancellation. Rows without
that final record are not a completed answer.

## 5. `functions`, `call_sites`, and `revisions`

These relations are an identity dictionary for observations, not a complete
version-control system for every BAML definition.

- `revision_id` answers **which exact compiled program produced this
  observation?** It is BLAKE3-256 over the source snapshot, compiler identity,
  and the currently included compiler options. The source snapshot hashes all
  project source files plus `baml.toml`, so any type-definition edit creates a
  new revision.
- `function_id` answers **which compact runtime function does this record
  name?** It is a dense integer meaningful only inside one revision.
- `definition_key` answers **which logical function is this across
  revisions?** A rename intentionally changes it.
- `local_definition_hash` answers **did this function's own compiled signature
  or body change?** It hashes the artifact's function kind, arity, nominal
  parameter/return/error types, canonicalized bytecode and referenced
  definition names. It does not recursively hash the contents of referenced
  types, functions, clients, prompts, or other definitions.
- `call_site_id` answers **which static source expression made this call?** It
  is not an invocation ID and is meaningful only inside one revision.

For example, adding a field to a class always changes `revision_id`. A function
that accepts the same named class may keep the same `definition_key` and
`local_definition_hash` when its own signature encoding and bytecode do not
change. Likewise, changing a callee's body does not automatically change its
caller's local hash. Equal local hashes therefore do not prove equal effective
behavior across revisions.

### Proposed schemas

#### `functions`

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `revision_id` | `id`; logical key with `function_id` | Scopes revision-local function IDs and joins the function to its compiled artifact. |
| `function_id` | `id` | Provides the compact runtime identity emitted in call evidence. |
| `definition_key` | `string?`; absent for synthetic/internal functions | Connects the same logical user function across revisions. |
| `local_definition_hash` | `bytes?`; artifact field `def_content_hash` | Detects a change to this function's own compiled signature or body without claiming that its dependency closure is unchanged. |
| `fqn` | `string`; fully qualified name | Gives an unambiguous user-facing name for display and grouping. |
| `display_name` | `string` | Gives a concise label for tree and list views where the full name is too noisy. |
| `source_path` | `string?` | Navigates user-defined functions to their source file; it is absent for functions without source. |
| `source_start` | `integer?` | Locates the exact start of the definition for editor navigation. |
| `source_end` | `integer?` | Bounds the definition for highlighting and disambiguating multiple definitions in one file. |
| `source_line` | `integer?` | Supports cheap human-readable display without converting byte offsets on every query. Remove it if clients can do that conversion reliably. |
| `kind` | `enum`; bytecode/native/system operation | Distinguishes execution kinds whose timing and behavior should not be compared as if they were identical. |
| `origin` | `enum`; user/companion/internal/builtin/generated | Lets users include or exclude framework-generated and internal work from analysis. |
| `capture_inputs` | `enum`; disabled/auto/enabled | Explains the effective input-capture intent for this function. |
| `capture_output` | `enum`; disabled/auto/enabled | Explains the effective output-capture intent for this function. |
| `capture_error` | `enum`; disabled/auto/enabled | Explains the effective error-capture intent for this function. |
| `promote_on_error` | `enum`; disabled/auto/enabled | Explains whether a failure should cause otherwise unretained call evidence to be promoted. |

#### `call_sites`

This is a target relation. The protobuf section and row shape exist, but the
current revision-dictionary builder emits no call-site rows. Do not expose
`retained_calls.call_site_id` as navigable until the producer and dictionary
population land together.

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `revision_id` | `id`; logical key with `call_site_id` | Scopes revision-local call-site IDs and connects them to the correct source snapshot. |
| `call_site_id` | `id` | Provides the compact identity emitted by retained calls for source navigation. |
| `source_path` | `string` | Identifies the file containing the call expression. |
| `source_start` | `integer` | Locates the start of the call expression for exact editor navigation. |
| `source_end` | `integer` | Bounds the expression for highlighting and disambiguation. |
| `source_line` | `integer` | Supports cheap human-readable display. Remove it if line lookup from offsets is reliably available to every client. |

#### `revisions`

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `revision_id` | `id`; logical key | Gives compiled program structure a stable identity and scopes runtime function/call-site IDs. |
| `source_snapshot_id` | `id` | Connects observations to the exact source snapshot users need to inspect. |
| `compiler_id` | `string` | Records which compiler produced the artifact so behavior can be reproduced and version differences investigated. |
| `capture_policy_version` | `integer` | Connects the revision to the policy semantics used by decoded function capture fields. |
| `identity_state` | `enum`; verified/fallback_legacy | Warns when cross-revision identity used a legacy fallback rather than verified artifact identity. |
| `first_seen_at` | `timestamp` | Supports revision discovery and ordering when source or deployment metadata is unavailable. |

### Why they exist

- **Rows:** one function or call site within a revision, or one compiled
  revision.
- **Growth:** compile-time program structure, not invocation volume.
- **Keep:** `revisions` and `functions` are required. `call_sites` becomes
  required when retained records emit call-site IDs and exact source navigation
  is part of the shipped surface.
- **Enables:** source navigation, capture-policy explanation, revision filters,
  and cross-revision comparison through `definition_key`.

The artifact dictionary also contains declared names, owner/lambda identity,
package/namespace parts, and a raw capture bitfield. P0 omits display-only
identity parts and exposes only decoded policy fields with a user question.

All three metadata relations are immutable. Reprocessing the same revision
must reproduce the same logical rows and hashes; a different row for the same
logical identity is an integrity conflict, not an update.

Do not add a public `compiler_options_hash` merely to duplicate identity. The
revision constructor currently commits to optimization level and
`emit_test_cases`; Q1 must audit every behavior-affecting compiler input and add
any missing input to `revision_id`. Expose decoded options later only when a
user-facing query needs to explain the difference.

### Did this function's own compiled definition change?

**English:** Show the local compiled-definition hash for the same logical
function in each revision. Different hashes prove its own signature or
bytecode changed. Equal hashes do not prove that referenced definitions stayed
the same.

```sql
SELECT
    revision_id,
    definition_key,
    local_definition_hash
FROM functions
WHERE definition_key = :definition_key
ORDER BY revision_id;
```

### How did the same logical function behave across revisions?

**English:** Compare volume, failures, and average direct time for the same
logical function across revisions.

```sql
SELECT
    revision_id,
    definition_key,
    SUM(calls_started) AS calls,
    SUM(calls_errored) AS failures,
    SUM(self_ns) / NULLIF(SUM(calls_started), 0) AS mean_self_ns_per_entry
FROM cct_population
WHERE definition_key = :definition_key
GROUP BY revision_id, definition_key
ORDER BY revision_id;
```

This is an investigation signal, not statistical proof of a regression. Keep
`revision_id` in the result: grouping only by `definition_key` would combine
different whole-program contexts.

## 6. `llm_population` — provisional

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `node_id`, `provider`, and `model` | Scopes usage to one run and supports run-level token accounting. |
| `node_id` | `id`; joins `cct_population` | Attributes LLM activity to the call-tree location that caused it. |
| `provider` | `string`; provisional | Disambiguates identical model names across providers. Remove it if Aaron's model supplies a single stable model identity instead. |
| `model` | `string` | Groups usage and errors by the model users selected. |
| `llm_calls` | `count` | Provides the invocation denominator for average usage and error rates. |
| `token_state` | `enum`; available/partial/unavailable | Distinguishes true zero-token usage from incomplete or missing provider measurements. |
| `input_tokens` | `count?` | Answers input-usage and cost questions when available; absence must remain distinct from zero. |
| `output_tokens` | `count?` | Answers output-usage and cost questions when available; absence must remain distinct from zero. |
| `provider_errors` | `count` | Separates failures returned by the provider from local parsing failures. |
| `parse_errors` | `count` | Measures calls whose provider response arrived but could not be parsed into the expected result. |

### Why it exists

- **Row:** one run/call-tree-location/provider/model combination.
- **Growth:** unique combinations, not LLM invocations.
- **Keep:** provisional; aggregate-only growth is acceptable.
- **Enables:** token and LLM-error summaries without scanning prompts or
  responses.

This schema matches current LLM functions and is expected to change with
Aaron's work. Do not freeze additional token classes or attempt/call semantics
before that lands. Current model evidence also does not cleanly expose a
separate provider identity, so `provider` is a required addition only if
Aaron's model retains provider/model as the public grouping.

Like `cct_population`, active usage arrives as immutable batched deltas and a
sealed run produces one immutable final row per logical key. The public
relation selects the final row or sums active deltas, never both.

### Which models used tokens or produced errors?

**English:** For one run, compare complete reported token use and errors by
provider/model.

```sql
SELECT
    provider,
    model,
    SUM(llm_calls) AS calls,
    SUM(input_tokens) AS total_input_tokens,
    SUM(output_tokens) AS total_output_tokens,
    SUM(provider_errors) AS provider_errors,
    SUM(parse_errors) AS parse_errors
FROM llm_population
WHERE run_id = :run_id
  AND token_state = 'available'
GROUP BY provider, model
ORDER BY total_input_tokens + total_output_tokens DESC;
```

This excludes partial/unavailable usage; a coverage query must account for
those rows separately.

## 7. `spawn_edges` and `spawn_instances` — conditional

### Proposed schemas

#### `spawn_edges`

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `edge_id` | Scopes the aggregate relationship to one run. |
| `edge_id` | `id` | Gives the parent-location/child-function relationship a stable join key for retained examples. |
| `parent_node_id` | `id` | Identifies which call-tree location initiated the child work. |
| `child_function_id` | `id` | Identifies the function being spawned through the run's revision dictionary. |
| `spawned` | `count` | Measures total fan-out and is the denominator for completion and error rates. |
| `completed` | `count` | Measures work that reached successful completion. |
| `errored` | `count` | Finds child relationships producing failures. |
| `cancelled` | `count` | Keeps cancelled work distinct from application errors. |
| `running_ns` | `duration_ns` | Measures time child work spent executing. Keep only if its accounting semantics can be made exact. |
| `awaiting_ns` | `duration_ns` | Measures time the parent spent waiting for child work, which is distinct from child execution time. |
| `retained_instances` | `count` | States how many exact spawn examples can actually be inspected. |
| `instances_dropped` | `count` | Prevents a selective exact-instance table from being mistaken for complete spawn history. |

#### `spawn_instances`

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `spawn_id` | Scopes the retained spawn to one run and joins run/revision context. |
| `spawn_id` | `id` | Gives the exact spawn a stable identity for inspection and causal links. |
| `edge_id` | `id`; joins `spawn_edges` | Connects the retained example to its complete aggregate relationship. |
| `thread_id` | `id` | Places the spawn on its logical execution thread for ordering and concurrency diagnosis. |
| `parent_call_id` | `id?` | Links to the exact initiating call when that call was retained. |
| `child_call_id` | `id?` | Links to the exact child call when that call was retained. |
| `status` | `enum`; pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned | Selects retained examples by lifecycle outcome. |
| `started_at` | `timestamp` | Orders retained spawns and places them on the run timeline. |
| `ended_at` | `timestamp?` | Distinguishes open from closed work and bounds its wall-clock interval. |
| `exact_window_ids` | `list<id>` | Links the spawn to every retained incident window that contains it. |
| `evidence_ids` | `list<id>` | Identifies the authoritative evidence used to reconstruct the spawn without exposing its storage location. |
| `evidence_state` | `enum`; available/incomplete/pending/lost/corrupt | Says whether the exact instance is trustworthy and inspectable rather than merely present as a row. |

### Why they exist

- **Rows:** one unique parent-location/child-function relationship, plus
  selected exact spawn instances.
- **Growth:** unique edges and policy-retained instances, not every spawn.
- **Keep:** only if concurrency diagnosis is P0.
- **Enables:** fan-out, failed/cancelled child work, outstanding work, and
  links to retained child evidence.

`spawn_edges` follows the same immutable delta/final pattern as
`cct_population`. A terminal `spawn_instances` row is immutable; a still-open
instance comes from the active overlay. Its window and evidence lists are
assembled from append-only provider link facts rather than updated arrays.

### Which child functions produced failed work?

**English:** For one run, show child functions with failed or cancelled work
and whether exact child instances were dropped.

```sql
SELECT
    se.child_function_id,
    f.fqn AS child_function,
    SUM(se.spawned) AS spawned,
    SUM(se.errored) AS failed,
    SUM(se.cancelled) AS cancelled,
    SUM(se.instances_dropped) AS instances_not_retained
FROM spawn_edges AS se
JOIN runs AS r ON r.run_id = se.run_id
JOIN functions AS f
  ON f.revision_id = r.revision_id
 AND f.function_id = se.child_function_id
WHERE se.run_id = :run_id
GROUP BY se.child_function_id, f.fqn
HAVING SUM(se.errored + se.cancelled) > 0
ORDER BY failed DESC, cancelled DESC;
```

## 8. `exact_windows`

The profiler's **tape** is bounded, rolling exact-event memory: new events enter
the tape and the oldest events are eventually overwritten. When an error,
manual action, or policy asks to preserve what happened, the profiler writes
the retained region as a durable dump. Explicit capture and the opt-in raw
stream can produce retained regions through the same logical model.

`exact_windows` is not the tape and does not contain an event row for every
call. It is the small searchable ledger of those preserved regions:

```text
runtime events -> bounded rolling tape -> retained dump in local evidence/S3
                                      -> one exact_windows metadata row
```

The detailed events stay in the dump identified by `evidence_id`. Calls decoded
from a retained region appear in `retained_calls`. A new ledger row is created
per preserved region, not per call or elapsed-time bucket; nearby dumps may
cover some of the same events.

### Proposed schema

| Column | Type / rule | Why this row field is necessary |
| --- | --- | --- |
| `run_id` | `id`; logical key with `window_id` | Scopes the retained evidence region to the run users are investigating. |
| `window_id` | `id` | Gives the retained region a stable identity for links from calls and spawns. |
| `session_id` | `id` | Connects the window to profiler-session evidence and supports recovery before all events bind cleanly to a run. |
| `source` | `enum`; recent_ring/flight_dump/raw/explicit | Explains which capture mechanism produced the window and what guarantees it can provide. |
| `trigger` | `enum`; error/manual/policy/other | Explains why detailed evidence was retained instead of discarded. |
| `trigger_node_id` | `id?` | Jumps from a policy/error trigger to the aggregate call-tree location that caused retention. |
| `trigger_call_id` | `id?` | Jumps to the exact triggering call when it was retained. |
| `started_at` | `timestamp` | Bounds the beginning of the retained event interval. |
| `ended_at` | `timestamp` | Bounds the end of the retained event interval. |
| `event_count` | `count` | Communicates evidence size and enables cost/budget checks without opening the detailed bytes. |
| `evidence_state` | `enum`; available/incomplete/pending/lost/corrupt | Tells users whether the detailed evidence can be trusted and read. |
| `incomplete_reasons` | `list<enum>`; evicted/budget_exhausted/truncated/unsupported | Explains every known reason a present window is incomplete. |
| `evidence_id` | `id`; logical evidence identity | Gives the retained detail a stable public identity; a provider-private mapping locates local/S3 bytes without putting the payload or storage locator in ClickHouse's public schema. |

### Why it exists

- **Row:** one retained region of detailed events.
- **Growth:** one row per preserved tape/dump region, not per call or clock
  tick. Detailed evidence bytes and decoded retained calls grow with the events
  inside those selected regions.
- **Keep:** yes as a small evidence ledger; detailed bytes remain outside the
  table.
- **Enables:** whether incident evidence exists, why it was retained, and
  whether it is complete.

The ClickHouse window row is inserted after the dump seals and is immutable.
`pending` can appear only through the active/control overlay before that row
exists. Later corruption, loss, or authorized erasure is represented by issue
or tombstone facts composed into the public `evidence_state`; it does not
rewrite the sealed window metadata.

### What detailed incident evidence was retained?

**English:** List detailed windows for a run and show whether each is complete.

```sql
SELECT
    window_id,
    trigger,
    started_at,
    ended_at,
    event_count,
    evidence_state,
    incomplete_reasons
FROM exact_windows
WHERE run_id = :run_id
ORDER BY started_at;
```

## Not required now: `cct_windows`

### Previously proposed schema

The table is rejected as a whole, but these were the intended roles of its
fields:

| Column | Type / rule | Why it was proposed |
| --- | --- | --- |
| `session_id` | `id` | Would scope buckets to one profiler session. |
| `epoch_id` | `id` | Would distinguish resets or counter epochs within a session. |
| `run_id` | `id?` | Would connect a bucket to a user-visible run when known. |
| `node_id` | `id` | Would identify the call-tree location measured by the bucket. |
| `window_started_at` | `timestamp` | Would define the beginning of the chart interval. |
| `window_ended_at` | `timestamp` | Would define the nominal end of the chart interval. |
| `calls_started` | `count`; bucket delta | Would show changes in call volume over time. |
| `calls_errored` | `count`; bucket delta | Would show changes in failures over time. |
| `inclusive_ns` | `duration_ns`; bucket delta | Would show changes in end-to-end time attributed to the location. |
| `self_ns` | `duration_ns`; bucket delta | Would show changes in direct execution time. |
| `await_ns` | `duration_ns`; bucket delta | Would show changes in waiting time. |
| `duration_histogram` | `list<count>`; bucket delta | Would support time-bounded tail-latency estimates. |
| `measured_through` | `timestamp` | Would distinguish the bucket's measurement watermark from its nominal end while the bucket was still open. |

### Why it is not justified

It enables historical “when did this spike?” charts, but grows as:

```text
active call-tree locations × elapsed time buckets
```

At 250 ms, one active location creates four rows per second. The open bucket is
also mutable. V1 already has complete totals in `cct_population`, current
updates through the private live path, and bounded incident evidence in
`exact_windows`.

Therefore `cct_windows` is not in the minimal catalog. Add it later only for a
measured historical workflow, as a coarse retention-limited derived view—not
authoritative or indefinite evidence.

## Minimal catalog

| Table | Decision | Growth driver |
| --- | --- | --- |
| `runs` | Required | Runs |
| `cct_population` | Required | Unique call-tree locations per run |
| `retained_calls` | Required and bounded | Retained calls |
| `evidence_issues` | Required and grouped | Source scopes containing issues |
| `functions` | Required metadata | Functions per revision |
| `call_sites` | Target metadata; producer not built | Static call expressions per revision, not invocations |
| `revisions` | Required metadata | Compiled revisions |
| `exact_windows` | Required ledger | Retained incidents/dumps |
| `llm_population` | Provisional | Unique run/location/provider/model combinations |
| `spawn_edges` | Conditional on concurrency P0 | Unique parent/child relationships |
| `spawn_instances` | Conditional on concurrency P0 | Retained child tasks |
| `cct_windows` | Excluded from minimal v1 | Active locations multiplied by time |

This is the public catalog. Provider-private active snapshots, aggregate
deltas, immutable relationship links, batch ledgers, and projection provenance
do not become additional public relations merely because the physical provider
uses them.

## Query rules

1. Use `runs` to find a run.
2. Use `cct_population` for complete all-call totals.
3. Use `retained_calls` only for selected exact evidence.
4. Filter on small columns before requesting values.
5. Group revisions with `definition_key`, not `function_id`.
6. Treat `local_definition_hash` as a local-change signal, not proof that the
   function's dependency closure is unchanged.
7. Check value states, `evidence_issues`, and `query_outcome` before claiming
   completeness.

## References

- [Canonical Project Studio ledger](README.md)
- [Query semantics](design/04-query-system.md)
- [Profiler](design/03-profiler.md)
- [Local artifacts and value store](design/storage/local-artifacts.md)
- [Hosted ClickHouse boundary](design/storage/clickhouse.md)
- [Delivery milestones](design/09-delivery-plan.md)

# Readers, Ignore:

## Decisions from this column pass

Remove:

- `runs.degraded` and free-form `runs.diagnostics`; typed evidence states and
  `evidence_issues` explain the problem.
- LLM/token totals from `runs`; `llm_population` is their one aggregate home.
- Precomputed display paths from `cct_population`; parent IDs and depth preserve
  the tree.
- Public physical value handles, artifact offsets, projection generations, and
  row hashes.
- `retained_calls.args`, `retained_calls.return`, and `retained_calls.error`
  from the resident schema; they remain virtual query fields resolved from
  local evidence or S3.
- Required public helper chains such as
  `baml_value_int(baml_value_at_path(...))`; DataFusion may use internal
  functions after lowering ordinary SQL syntax.
- Physical sequence/byte ranges from `exact_windows`; `evidence_id` hides that
  layout.

Add:

- Run entrypoint identity and exact duration.
- `retained_calls.node_id` to connect an exact call to its complete summary.
- `call_sites` because `retained_calls.call_site_id` otherwise has no public
  target once call-site IDs are emitted; the current dictionary section is
  present but empty.
- First/last timestamps on grouped loss summaries.
- LLM provider identity and token-availability state.
- Stable IDs on spawn instances and exact-evidence windows.
- Whole-value equality and nested value traversal through ordinary SQL
  operators and subscripts.
- The public name `local_definition_hash` for the artifact's existing
  `def_content_hash`, so an implementation-local signal is not mistaken for a
  dependency-aware behavior version.

Still unresolved:

- Aaron's LLM work will change `llm_population`.
- Release, deployment, service, git, and bounded application-tag filters need a
  proper dimension model—not a free-form metadata blob.
- Logs are one-to-many with calls. Add a bounded `retained_logs` relation or
  remove public SQL log inspection from P0.
- `exact_windows` is an evidence ledger, not an event table. Keep detailed
  event reads on the bounded private RPC unless `retained_events` is designed.

Resolved by the Q1 freeze (see the
[implementation notes](design/12-implementation-notes.md)): `args` is a
named-argument object; subscripts are name-keyed for objects and
zero-based for lists; an available value's absent path or incompatible
leaf is an ordinary non-match distinct from typed unavailability; and
`retained_calls` carries no `process_id`/`engine_id` — `run_id` +
`call_id` is the frozen identity scope.
