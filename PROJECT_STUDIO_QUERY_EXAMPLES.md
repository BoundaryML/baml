# Project Studio data model and query examples

This document starts with the questions Project Studio must answer, derives the
smallest useful set of tables, and then shows representative SQL. It is meant
to be readable without the rest of the design corpus.

> **Status:** this is the target SQL model, not a command surface available on
> this branch today. Table names omit version suffixes for readability. Exact
> column types, lifecycle status spellings, parameter binding, and BAML
> value-function names freeze in milestone Q1. In the examples, `:name` means a
> value supplied by the caller.

## Start with the questions

Project Studio needs to answer four different kinds of questions:

1. **What happened across every call?** For example, which functions failed or
   consumed the most time?
2. **Which exact call should I inspect?** For example, what arguments produced
   this error?
3. **What information is missing?** For example, was a value not captured,
   redacted, lost, or still pending?
4. **Which source code produced the behavior?** For example, did the same
   logical function change across revisions?

One table cannot answer all four questions efficiently. In particular, storing
one database row for every function invocation would make storage and ingest
grow directly with application traffic. Project Studio instead keeps complete
small summaries for all calls and selectively retains individual calls for
debugging.

## Where the data lives

| Place | What belongs there | Why |
| --- | --- | --- |
| Local `.baml` files | Authoritative local run evidence and captured values | Profiling does not require a database or network request on the application path |
| S3 | Authoritative uploaded evidence, arguments, returns, errors, and log bodies | Large or sensitive bytes remain immutable and independently recoverable |
| ClickHouse | Rebuildable small facts used for filtering, joins, counts, grouping, and ordering | It can answer fleet-scale analytical questions without storing customer value content |
| PostgreSQL | Ownership, upload commitments, projection state, audit, retention, and deletion workflow | Transactional workflow state is different from analytical data |
| Public SQL layer | The logical tables below | It presents the same meanings locally and hosted, regardless of the physical storage used |

Selecting `args`, `return`, or `error` does not mean those bodies live in
ClickHouse. The query first narrows the candidate calls using small fields,
then loads the requested value from local files or S3.

## The growth test

Before adding a table, answer:

- What does one row represent?
- What event creates another row?
- Does it grow with calls, unique program structure, retained evidence, or
  elapsed time?
- What user question becomes possible only because this table exists?
- Can its final rows be immutable and rebuilt from authoritative evidence?

A table is not justified merely because it appeared in an older design.

For hosted data, completed summary rows should be immutable projections.
Running state may change, but it should use a bounded active representation or
the live-update path—not create permanent history on every refresh. The exact
physical representation of running rows remains implementation freeze work.

## Core table 1: `runs`

### Why it exists

A user needs a cheap way to discover runs before opening one. Deriving this
list from function summaries would lose empty runs, lifecycle state, revision
identity, and evidence-health information.

### Row and growth

- **One row:** one program run.
- **Growth:** one small row per run, not per function call.
- **Keep it:** yes. It is the entry point for nearly every workflow and its
  growth is proportional to user-visible runs.
- **It buys us:** recent-run lists, running/failed/cancelled state, revision
  filters, error totals, token totals, and evidence-health filters.

### Which recent runs had problems?

**English:** List the newest runs that recorded an error or whose evidence was
marked degraded.

```sql
SELECT
    run_id,
    created_ms,
    status,
    revision_id,
    total_calls,
    total_errors,
    degraded
FROM runs
WHERE created_ms >= :from_ms
  AND (total_errors > 0 OR degraded = TRUE)
ORDER BY created_ms DESC
LIMIT 100;
```

This reads only the small run rows. No argument, response, error body, or S3
object is needed.

## Core table 2: `cct_population`

The name is historical. Read it as **call-tree summaries**.

### Why it exists

We need complete counts and timing across every invocation, but we explicitly
do not want one row per invocation. This table records accumulated counters at
each distinct location in the nested function-call tree.

If `Checkout` calls `PriceItem` one million times from the same place, those
invocations update the same summary row. If `PriceItem` is also called from a
different parent, that is a different call-tree location and therefore a
different row.

### Row and growth

- **One row:** one distinct call-tree location within one run.
- **Growth:** unique call paths, not repeated invocations.
- **Risk:** highly dynamic call paths can still produce many rows. Epoch
  rotation, memory bounds, and path-cardinality tests must keep that honest.
- **Keep it:** yes. This is what makes complete all-call analysis possible
  without traffic-proportional analytical storage.
- **It buys us:** complete call/error counts, inclusive time, direct execution
  time, waiting time, and duration distributions by call location or function.

In the SQL, `fqn` is the full function name. `definition_key` is the stable
identity used to recognize the same logical function across revisions.

### Which functions fail most often?

**English:** Across every call in a time range, rank functions by the number of
errors and show their failure rate.

```sql
SELECT
    definition_key,
    fqn AS function_name,
    SUM(enters) AS calls,
    SUM(ends_err) AS failures,
    1.0 * SUM(ends_err) / NULLIF(SUM(enters), 0) AS failure_rate
FROM cct_population
WHERE run_id IN (
    SELECT run_id
    FROM runs
    WHERE created_ms >= :from_ms
      AND created_ms < :to_ms
)
GROUP BY definition_key, fqn
HAVING SUM(ends_err) > 0
ORDER BY failures DESC
LIMIT 50;
```

The runtime already maintains these totals. The query does not need individual
call rows or captured values.

### Where did one run spend its time?

**English:** Rank functions in one run by their direct execution time and also
show inclusive and waiting time.

```sql
SELECT
    definition_key,
    fqn AS function_name,
    SUM(enters) AS calls,
    SUM(total_ns) AS inclusive_ns,
    SUM(self_ns) AS self_ns,
    SUM(await_ns) AS await_ns,
    SUM(self_ns) / NULLIF(SUM(enters), 0) AS mean_self_ns_per_entry
FROM cct_population
WHERE run_id = :run_id
GROUP BY definition_key, fqn
ORDER BY self_ns DESC
LIMIT 50;
```

The mean is a directional summary, not a percentile or proof of a regression.

## Core table 3: `retained_calls`

### Why it exists

Summary counters can identify a problematic function, but they cannot show the
exact arguments, return, error, thread, or parent call. This table is the
bridge from a summary to selected exact evidence.

It is intentionally incomplete: only calls selected by capture policy, an
incident window, or explicit promotion are discoverable as individual rows.

### Row and growth

- **One row:** one individually retained function call.
- **Growth:** retained calls, not all calls.
- **Risk:** “capture everything” would recreate traffic-proportional storage.
  Capture budgets and retention policy must bound this table.
- **Keep it:** yes, provided the retention boundary remains explicit.
- **It buys us:** exact-call lists, call relationships, targeted value
  inspection, and value predicates over a deliberately narrowed cohort.

The row stores small identifiers, status, timing, provenance, and availability
information. Hosted arguments, returns, errors, and logs remain in S3.

### Which retained failures should I inspect?

**English:** Within one run, list the slowest failed calls for which individual
evidence was retained.

```sql
SELECT
    call_id,
    run_id,
    definition_key,
    duration,
    status
FROM retained_calls
WHERE run_id = :run_id
  AND status = 'errored'
ORDER BY duration DESC
LIMIT 100;
```

This returns **retained failures**, not every failure. Use
`cct_population` for the complete failure count.

### What did one retained call receive and produce?

**English:** Load the captured arguments, return value, and error for one known
call.

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

The IDs reduce the candidate set to one call before any large value is loaded.
BAML's deduplicated value files and bounded reader already exist locally; the
hosted design performs an authorized S3 read.

### Which retained calls contain a particular value?

**English:** Within one run and one function, find up to 100 retained calls
whose first argument contains a customer age of at least 30.

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

The engine applies the cheap run/function filters first, then loads distinct
candidate values in bounded batches. The final limit is applied only after the
value condition has been checked.

This supports targeted debugging. It does not justify a permanent plaintext,
full-text, or decoded-field index over all customer values in ClickHouse.

## Core table 4: `capture_losses`

### Why it exists

“No value” is ambiguous. It can mean the value was absent, not captured,
redacted, lost, truncated, corrupt, unsupported, or still pending. Without an
explicit loss table, Project Studio would produce confident but incorrect
answers.

### Row and growth

- **One row:** one summarized loss/degradation fact for a run or source.
- **Growth:** distinct recorded loss facts. Repeated identical losses should
  increase a count rather than create one row per lost invocation.
- **Risk:** if every loss becomes its own row, this table recreates the same
  traffic-growth problem. The grouping contract must be frozen in Q1.
- **Keep it:** yes. Correctness requires it.
- **It buys us:** evidence-quality summaries and a defensible answer to “is
  this result complete?”

### Is the evidence complete enough to trust?

**English:** Summarize every recorded reason evidence is missing or degraded
for one run.

```sql
SELECT
    kind,
    reason,
    SUM(count) AS affected_records
FROM capture_losses
WHERE run_id = :run_id
GROUP BY kind, reason
ORDER BY affected_records DESC;
```

Some current profiler paths record counters or markers but do not yet persist
diagnostics consistently. Closing that gap is milestone C1 work.

Every query also sends a separate final completion record called
`query_outcome`. It reports which data snapshot was used, whether required
values were unavailable, and whether the query hit a budget or was cancelled.
Rows without that final record are not a successfully completed answer.

## Supporting tables

These tables support the core model but are smaller or feature-specific.

### `functions` and `revisions` — keep

- **Why:** numeric function IDs only make sense inside one compiled revision.
  These tables connect behavior to source and provide `definition_key` for
  cross-revision comparison.
- **Growth:** functions per compiled revision and number of revisions, not
  runtime call volume.
- **Why acceptable:** small metadata with high explanatory value.
- **Queries enabled:** source navigation, revision filters, and comparison of
  the same function across builds.

**English:** Compare call volume, failures, and average direct execution time
for the same logical function across revisions.

```sql
SELECT
    revision_id,
    definition_key,
    SUM(enters) AS calls,
    SUM(ends_err) AS failures,
    SUM(self_ns) / NULLIF(SUM(enters), 0) AS mean_self_ns_per_entry
FROM cct_population
WHERE definition_key = :definition_key
GROUP BY revision_id, definition_key
ORDER BY revision_id;
```

Treat this as a signal to investigate. A credible regression claim needs
distribution and sample checks beyond this summary.

### `llm_population` — provisional

- **Why:** summarize LLM calls, tokens, and provider/parser errors by run,
  call-tree location, and model without scanning prompts or responses.
- **Growth:** unique run/location/model combinations, not LLM invocations.
- **Why acceptable:** it stays aggregate-only.
- **Important:** this is designed around the current LLM functions and is
  expected to change with Aaron's LLM work. Its row model and columns are not
  frozen.

**English:** For one run, compare aggregate token use and errors by model.

```sql
SELECT
    model,
    SUM(llm_calls) AS calls,
    SUM(tokens_in) AS input_tokens,
    SUM(tokens_out) AS output_tokens,
    SUM(provider_errors) AS provider_errors,
    SUM(parse_errors) AS parse_errors
FROM llm_population
WHERE run_id = :run_id
GROUP BY model
ORDER BY input_tokens + output_tokens DESC;
```

This query reads summary fields only. It does not read prompt or response
bodies from S3.

### `spawn_edges` and `spawn_instances` — keep if concurrency is in P0

- `spawn_edges` summarizes each distinct parent-location-to-child-function
  relationship. Repeated spawns update counters, so growth follows unique
  relationships rather than spawn count.
- `spawn_instances` contains only individually retained child tasks. Its
  growth must follow the same capture and retention bounds as
  `retained_calls`.
- They enable fan-out, failed-child, cancelled-child, and outstanding-work
  questions. If those questions leave P0, these tables can leave the initial
  catalog too.
- Exact public columns are not frozen, so this document does not invent a SQL
  example yet.

### `exact_windows` — keep as a small evidence ledger

- **Why:** record that a bounded region of detailed events was retained, why it
  was retained, where its bytes live, and whether anything was evicted.
- **Growth:** retained incident windows or explicit dumps, not clock time.
- **Why acceptable:** capture policy bounds the number and size; detailed
  events stay in local files or S3.
- **Queries enabled:** “is detailed evidence available around this failure?”
  and “was part of the retained incident window evicted?”

## Table not required now: `cct_windows`

A `cct_windows` row was proposed as counters for one call-tree location during
one short time bucket.

It would enable historical charts such as “when did errors spike?” But its
growth is:

```text
active call-tree locations × elapsed time buckets
```

At a 250 ms bucket size, one continuously active location creates four rows per
second. Thousands of active locations in a long-running service create a large
permanent time-series projection even though no individual calls are stored.
The currently open bucket also changes as calls arrive, so it is not naturally
an immutable final row.

The v1 product already has other ways to answer the immediate needs:

- `cct_population` answers complete totals.
- The private live-update path powers current charts and counters.
- `exact_windows` retains bounded detailed evidence around selected incidents.

Therefore `cct_windows` is not part of the minimal v1 catalog. If a concrete
historical time-series workflow later justifies it, add a measured, coarse,
retention-limited derived view. Do not treat it as authoritative evidence or
retain it indefinitely by default.

## Other things that are not public analytical tables

- **Every function invocation:** rejected because it grows directly with
  traffic. Individual rows exist only in `retained_calls`.
- **Decoded value fields or searchable customer text:** rejected for
  ClickHouse. Values are loaded from local files or S3 for bounded candidate
  calls.
- **Running-versus-terminal physical tables:** this may be a useful internal
  implementation split, but it should not become two public concepts unless a
  user question requires it.
- **Raw ClickHouse tables:** physical schemas remain private so local and
  hosted queries can share one logical contract.

## Minimal catalog

| Table | Decision | Growth driver |
| --- | --- | --- |
| `runs` | Required | Program runs |
| `cct_population` | Required | Unique call-tree locations per run |
| `retained_calls` | Required and policy-bounded | Individually retained calls |
| `capture_losses` | Required and summarized | Distinct loss/degradation facts |
| `functions` | Required metadata | Functions per revision |
| `revisions` | Required metadata | Compiled revisions |
| `exact_windows` | Required evidence ledger | Retained incidents/dumps |
| `llm_population` | Provisional pending Aaron's changes | Unique run/location/model combinations |
| `spawn_edges` | Required only if concurrency is P0 | Unique parent/child relationships |
| `spawn_instances` | Required only if concurrency is P0 | Individually retained spawned tasks |
| `cct_windows` | Not required for minimal v1 | Active locations multiplied by time buckets |

## Query-writing rules

1. Use `runs` to find a run.
2. Use `cct_population` for complete all-call totals.
3. Use `retained_calls` only for individually retained evidence; never present
   its count as the count of all calls.
4. Filter on run, time, function, status, and IDs before requesting arguments,
   returns, errors, or logs.
5. Group across revisions with `definition_key`, not `function_id`.
6. Check `capture_losses`, per-value availability, and `query_outcome` before
   claiming an answer is complete.

## Design references

- [Canonical Project Studio ledger](CANONICAL/README.md)
- [Query semantics and logical catalog](CANONICAL/design/04-query-system.md)
- [Profiler and measured substrate](CANONICAL/design/03-profiler.md)
- [Local artifacts and value store](CANONICAL/design/storage/local-artifacts.md)
- [Hosted ClickHouse boundary](CANONICAL/design/storage/clickhouse.md)
- [Delivery milestones](CANONICAL/design/09-delivery-plan.md)
