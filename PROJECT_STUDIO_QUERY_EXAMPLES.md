# Project Studio data model and query examples

This document derives the smallest useful Project Studio table model from the
questions users need to answer. Each table includes its proposed public schema,
why it exists, how it grows, and the queries it makes possible.

The query presentation follows the local query-engine
[example](https://github.com/BoundaryML/baml/blob/codex/local-query-engine/baml_language/crates/baml_query/EXAMPLE.md),
but the tables below follow Project Studio's aggregate-plus-retained-evidence
model rather than the prototype's all-call table.

> **Status:** this is a proposed target SQL model, not a command surface on this
> branch today. Names omit version suffixes for readability. Exact SQL types,
> enum spellings, parameter binding, and BAML value-function names freeze in
> milestone Q1. `:name` means a value supplied by the caller.

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
| Public SQL | The logical tables below, independent of physical storage |

Selecting `args`, `return`, or `error` does not mean the value lives in
ClickHouse. The query narrows candidates using small columns, then loads the
requested value from local files or S3.

## Schema rules

Logical types used below:

- `id` — opaque stable identifier;
- `timestamp` — UTC instant;
- `count` — non-negative 64-bit counter;
- `duration_ns` — non-negative nanosecond duration;
- `enum` — documented closed set;
- `value` — BAML value loaded on demand;
- `list<T>` — bounded list of `T`; and
- `?` — nullable.

Physical tables still need tenant/project scope, projection generation, row
hashes, source ranges, and opaque value handles. Those are provider machinery,
not public query columns. Query snapshot and projection-watermark information
belongs in `query_outcome` rather than every row.

## Decisions from this column pass

Remove:

- `runs.degraded` and free-form `runs.diagnostics`; typed evidence states and
  `evidence_issues` explain the problem.
- LLM/token totals from `runs`; `llm_population` is their one aggregate home.
- Precomputed display paths from `cct_population`; parent IDs and depth preserve
  the tree.
- Public physical value handles, artifact offsets, projection generations, and
  row hashes.
- Physical sequence/byte ranges from `exact_windows`; `evidence_id` hides that
  layout.

Add:

- Run entrypoint identity and exact duration.
- `retained_calls.node_id` to connect an exact call to its complete summary.
- `call_sites` because `retained_calls.call_site_id` otherwise has no public
  target.
- First/last timestamps on grouped loss summaries.
- LLM provider identity and token-availability state.
- Stable IDs on spawn instances and exact-evidence windows.

Still unresolved:

- Remove `process_id` or `engine_id` from `retained_calls` if Q1 proves
  `run_id` already scopes call identity.
- Aaron's LLM work will change `llm_population`.
- Release, deployment, service, git, and bounded application-tag filters need a
  proper dimension model—not a free-form metadata blob.
- Logs are one-to-many with calls. Add a bounded `retained_logs` relation or
  remove public SQL log inspection from P0.
- `exact_windows` is an evidence ledger, not an event table. Keep detailed
  event reads on the bounded private RPC unless `retained_events` is designed.

## 1. `runs`

### Proposed schema

```text
run_id             id          primary key
started_at         timestamp
ended_at           timestamp?  absent while running
duration_ns        duration_ns exact elapsed or so-far time
status             enum        pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned
revision_id        id          joins revisions
entry_function_id  id?         root BAML function, when applicable
entrypoint         string      command/test/function shown to the user
total_calls        count       all function invocations
total_errors       count       all errored invocations
structure_state    enum        complete/incomplete/pending/lost
value_state        enum        complete/partial/pending/not_captured/lost
integrity_state    enum        verified/unverified/corrupt/conflicting
projection_state   enum        pending/active/delayed/failed/rebuilding
retention_state    enum        retained/partially_retained/erased
```

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

```text
run_id              id           primary key with node_id
node_id             id           call-tree location within the run
parent_node_id      id?          absent for the root
depth               integer      avoids recursive work for common tree views
function_id         id           identity within one revision
revision_id         id           repeated for fast grouping
definition_key      string?      stable identity; absent for synthetic functions
definition_hash     bytes?       distinguishes changed implementations
fqn                  string       full function name
calls_started       count
calls_succeeded     count
calls_errored       count
calls_cancelled     count
calls_exited        count        other explicit terminal exits
inclusive_ns        duration_ns  function plus nested calls
self_ns             duration_ns  direct execution only
await_ns            duration_ns  suspended/waiting time
duration_histogram  list<count>  fixed catalog-owned duration buckets
```

### Why it exists

- **Row:** one distinct call-tree location within one run.
- **Growth:** unique call paths, not repeated invocations.
- **Keep:** yes; this provides complete all-call analysis without one row per
  invocation.
- **Enables:** complete call/error counts, inclusive/direct/waiting time, and
  duration distributions.

One million calls from the same parent to the same function update one row. A
different parent creates another row. Highly dynamic paths can still grow the
table, so path-count and memory tests remain release gates.

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

```text
run_id                  id          primary key with call_id
call_id                 id
parent_call_id          id?         parent may not itself be retained
node_id                 id          joins cct_population
process_id              id          retain only if required by identity scope
engine_id               id          retain only if required by identity scope
thread_id               id          logical execution thread
definition_key          string?     duplicated for common function filtering
call_site_id            id?         joins call_sites through the run revision
started_at              timestamp
ended_at                timestamp?  absent while running
duration_ns             duration_ns exact monotonic or so-far duration
status                  enum        pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned
retention_reasons       list<enum>  policy/incident/promotion/explicit
exact_window_ids        list<id>    a call may appear in multiple windows
evidence_ids            list<id>    logical authoritative-evidence references
capture_policy_version  integer
args_state              enum        available/pending/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported
return_state            enum        available/pending/not_applicable/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported
error_state             enum        available/pending/not_applicable/not_captured/omitted/redacted/lost/truncated/corrupt/unsupported
args                     value?      loaded on demand
return                   value?      loaded on demand
error                    value?      loaded on demand
```

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
identity collisions.

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

The IDs narrow the candidate set before local/S3 value loading.

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
  AND baml_value_int(
        baml_value_at_path(args, baml_path('arg[0].customer.age'))
      ) >= 30
LIMIT 100;
```

Cheap run/function filters execute first. Values load in bounded, deduplicated
batches, and the limit applies only after the value condition.

## 4. `evidence_issues`

### Proposed schema

```text
issue_id        id          primary key for one sealed summary
run_id          id?         absent before run binding
session_id      id?         absent for non-runtime issues
evidence_id     id?         sealed evidence range summarized
source          enum        profiler/value_capture/uploader/projector/retention
kind            enum        evidence affected
reason          enum        typed cause of the issue
count           count       affected facts
first_seen_at   timestamp
last_seen_at    timestamp
policy_version  integer?
```

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

### Proposed schemas

```text
functions
  revision_id      id      primary key with function_id
  function_id      id
  definition_key   string? absent for synthetic/internal functions
  definition_hash  bytes?
  fqn              string
  display_name     string
  source_path      string?
  source_start     integer?
  source_end       integer?
  source_line      integer?
  kind             enum    bytecode/native/system operation
  origin           enum    user/companion/internal/builtin/generated
  capture_inputs   enum    disabled/auto/enabled
  capture_output   enum    disabled/auto/enabled
  capture_error    enum    disabled/auto/enabled
  promote_on_error enum    disabled/auto/enabled

call_sites
  revision_id  id       primary key with call_site_id
  call_site_id id
  source_path  string
  source_start integer
  source_end   integer
  source_line  integer

revisions
  revision_id            id         primary key
  source_snapshot_id     id
  compiler_id            string
  compiler_options_hash  bytes?
  capture_policy_version integer
  identity_state         enum       verified/fallback_legacy
  first_seen_at          timestamp
```

### Why they exist

- **Rows:** one function, call site, or compiled revision.
- **Growth:** compile-time program structure, not invocation volume.
- **Keep:** yes; runtime IDs are revision-local and meaningless without these
  tables.
- **Enables:** source navigation, capture-policy explanation, revision filters,
  and cross-revision comparison through `definition_key`.

The artifact dictionary also contains declared names, owner/lambda identity,
package/namespace parts, and a raw capture bitfield. P0 omits display-only
identity parts and exposes only decoded policy fields with a user question.

The current revision dictionary does not emit `compiler_options_hash`. Either
add it to the authoritative revision evidence or prove `revision_id` already
commits to every behavior-affecting option and remove this public column.

### Did a function change across revisions?

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

This is an investigation signal, not statistical proof of a regression.

## 6. `llm_population` — provisional

### Proposed schema

```text
run_id          id      primary key with node_id/provider/model
node_id         id      joins cct_population
provider        string  model name alone is ambiguous
model           string
llm_calls       count
token_state     enum    available/partial/unavailable
input_tokens    count?  absent differs from zero
output_tokens   count?  absent differs from zero
provider_errors count
parse_errors    count
```

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

```text
spawn_edges
  run_id             id          primary key with edge_id
  edge_id            id
  parent_node_id     id
  child_function_id  id
  spawned            count
  completed          count
  errored            count
  cancelled          count
  running_ns         duration_ns
  awaiting_ns        duration_ns
  retained_instances count
  instances_dropped  count

spawn_instances
  run_id           id          primary key with spawn_id
  spawn_id         id
  edge_id          id          joins spawn_edges
  thread_id        id
  parent_call_id   id?
  child_call_id    id?
  status           enum        pending/running/waiting/succeeded/failed/cancelled/panicked/abandoned
  started_at       timestamp
  ended_at         timestamp?
  exact_window_ids list<id>
  evidence_ids     list<id>
  evidence_state   enum        available/incomplete/pending/lost/corrupt
```

### Why they exist

- **Rows:** one unique parent-location/child-function relationship, plus
  selected exact spawn instances.
- **Growth:** unique edges and policy-retained instances, not every spawn.
- **Keep:** only if concurrency diagnosis is P0.
- **Enables:** fan-out, failed/cancelled child work, outstanding work, and
  links to retained child evidence.

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

### Proposed schema

```text
run_id              id          primary key with window_id
window_id           id
session_id          id
source              enum        recent_ring/flight_dump/raw/explicit
trigger             enum        error/manual/policy/other
trigger_node_id     id?
trigger_call_id     id?
started_at          timestamp
ended_at            timestamp
event_count         count
evidence_state      enum        available/incomplete/pending/lost/corrupt
incomplete_reasons  list<enum>  evicted/budget_exhausted/truncated/unsupported
evidence_id         id          logical reference to local/S3 bytes
```

### Why it exists

- **Row:** one retained region of detailed events.
- **Growth:** triggered/manual retained regions, not clock time.
- **Keep:** yes as a small evidence ledger; detailed bytes remain outside the
  table.
- **Enables:** whether incident evidence exists, why it was retained, and
  whether it is complete.

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

```text
session_id         id
epoch_id           id
run_id             id?
node_id            id
window_started_at  timestamp
window_ended_at    timestamp
calls_started      count       bucket delta
calls_errored      count       bucket delta
inclusive_ns       duration_ns bucket delta
self_ns            duration_ns bucket delta
await_ns           duration_ns bucket delta
duration_histogram list<count> bucket delta
measured_through   timestamp
```

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
| `call_sites` | Required metadata | Call sites per revision |
| `revisions` | Required metadata | Compiled revisions |
| `exact_windows` | Required ledger | Retained incidents/dumps |
| `llm_population` | Provisional | Unique run/location/provider/model combinations |
| `spawn_edges` | Conditional on concurrency P0 | Unique parent/child relationships |
| `spawn_instances` | Conditional on concurrency P0 | Retained child tasks |
| `cct_windows` | Excluded from minimal v1 | Active locations multiplied by time |

## Query rules

1. Use `runs` to find a run.
2. Use `cct_population` for complete all-call totals.
3. Use `retained_calls` only for selected exact evidence.
4. Filter on small columns before requesting values.
5. Group revisions with `definition_key`, not `function_id`.
6. Check value states, `evidence_issues`, and `query_outcome` before claiming
   completeness.

## References

- [Canonical Project Studio ledger](CANONICAL/README.md)
- [Query semantics](CANONICAL/design/04-query-system.md)
- [Profiler](CANONICAL/design/03-profiler.md)
- [Local artifacts and value store](CANONICAL/design/storage/local-artifacts.md)
- [Hosted ClickHouse boundary](CANONICAL/design/storage/clickhouse.md)
- [Delivery milestones](CANONICAL/design/09-delivery-plan.md)
