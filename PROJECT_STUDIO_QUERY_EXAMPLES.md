# Project Studio query examples

This is the short, shareable introduction to the questions Project Studio is
designed to answer. It follows the shape of the local query-engine
[example](https://github.com/BoundaryML/baml/blob/codex/local-query-engine/baml_language/crates/baml_query/EXAMPLE.md),
but uses the canonical Project Studio model rather than the prototype's
all-call `function_calls` table.

> **Status:** these are target v1 SQL queries, not commands that work on this
> branch today. The relation names and grain rules are settled. Exact column
> types, lifecycle enum spellings, parameter binding, and BAML value-function
> names freeze in milestone Q1. In the examples, `:name` denotes a bound
> parameter.

## The three query shapes

| Shape | Use it for | Why it can be fast | Readiness |
| --- | --- | --- | --- |
| Population aggregates | Counts, failures, time, tokens, and hot contexts across every call | The profiler folds calls into compact CCT/run aggregates instead of writing one analytical row per call | Profiler/fold substrate is built and measured; the public SQL provider is Q1/Q2 work |
| Retained-instance metadata | Finding specific calls that capture policy retained | Filters use resident IDs, status, function identity, and time before any value is read | Logical contract is settled; local and hosted providers are Q2/H2 work |
| Hydrated values | Inspecting or filtering retained arguments, returns, and errors | The planner narrows candidates first, deduplicates value handles, and reads canonical CAS values in bounded batches | Canonical codec/CAS exists and the prototype demonstrates lazy hydration; production ValueResolvers are Q2/H2 work |

Population and retained-instance answers are intentionally different. A row
in `cct_population_v1` contributes to complete aggregate totals. A row in
`retained_calls_v1` exists only when an individual execution was retained.

### What backs the viability claims

- The current profiler has reported 74.4 ns end-to-end overhead per call,
  47.8–48.6 ns for the CCT hot-loop pair, a 4.5 KiB result after five million
  calls, 2.62 ms completed-run first-frame open, and 34.3 MiB consumer RSS
  under sustained load on one development machine. These are architecture
  evidence and regression seeds, not portable SLOs.
- The canonical BAML value codec, content-addressed packs, deduplication,
  garbage collection, and budgeted reads are implemented locally.
- The linked DataFusion prototype demonstrates resident filter pushdown, lazy
  value hydration, residual predicates, final-limit placement, cancellation,
  and budgets. Its physical `function_calls`/SQLite/JSON-blob model is not the
  production contract.
- Production local SQL and the hosted ClickHouse/S3 path are designed and
  gated in Q1, Q2, and H2; they are not implemented on this branch.

## 1. Which recent runs had problems?

```sql
SELECT
    run_id,
    created_ms,
    status,
    revision_id,
    total_calls,
    total_errors,
    degraded
FROM runs_v1
WHERE created_ms >= :from_ms
  AND (total_errors > 0 OR degraded = TRUE)
ORDER BY created_ms DESC
LIMIT 100;
```

**English:** List the newest runs that recorded at least one error or whose
evidence was marked degraded. This is the first query to use for incident
triage.

**Why it is viable:** `runs_v1` has one resident row per run. The query reads
no customer values and can push its filter, ordering, and limit to a local
resident provider or hosted ClickHouse.

## 2. Which functions fail most often?

```sql
SELECT
    definition_key,
    fqn,
    SUM(enters) AS calls,
    SUM(ends_err) AS failures,
    1.0 * SUM(ends_err) / NULLIF(SUM(enters), 0) AS failure_rate
FROM cct_population_v1
WHERE run_id IN (
    SELECT run_id
    FROM runs_v1
    WHERE created_ms >= :from_ms
      AND created_ms < :to_ms
)
GROUP BY definition_key, fqn
HAVING SUM(ends_err) > 0
ORDER BY failures DESC
LIMIT 50;
```

**English:** Across every call in the selected time window, rank functions by
how many executions ended in error and show their failure rate.

**Why it is viable:** This is a population-true aggregation over compact CCT
totals; it does not need one row per invocation or any value hydration.
`definition_key` is stable across revisions, unlike a revision-local
`function_id`.

## 3. Where is execution time being spent?

```sql
SELECT
    definition_key,
    fqn,
    SUM(enters) AS calls,
    SUM(total_ns) AS inclusive_ns,
    SUM(self_ns) AS self_ns,
    SUM(await_ns) AS await_ns,
    SUM(self_ns) / NULLIF(SUM(enters), 0) AS mean_self_ns_per_entry
FROM cct_population_v1
WHERE run_id = :run_id
GROUP BY definition_key, fqn
ORDER BY self_ns DESC
LIMIT 50;
```

**English:** For one run, show the functions responsible for the most direct
work, along with inclusive and waiting time. This identifies where to begin a
performance investigation.

**Why it is viable:** The profiler maintains these counters on the structural
path and folds them into compact aggregate rows. No retained call or captured
argument is required. The mean is a directional summary, not a percentile or
proof of a regression.

## 4. Did a function change across revisions?

```sql
SELECT
    revision_id,
    definition_key,
    SUM(enters) AS calls,
    SUM(ends_err) AS failures,
    SUM(self_ns) / NULLIF(SUM(enters), 0) AS mean_self_ns_per_entry
FROM cct_population_v1
WHERE definition_key = :definition_key
GROUP BY revision_id, definition_key
ORDER BY revision_id;
```

**English:** Compare call volume, failures, and average direct execution time
for the same logical function across compiled revisions.

**Why it is viable:** Cross-revision identity is a first-class catalog rule,
and the metrics are already population aggregates. Treat the result as a
signal to investigate; a statistically credible regression claim needs
distribution/sample checks beyond this summary.

## 5. Which runs used the most LLM tokens?

```sql
SELECT
    run_id,
    created_ms,
    revision_id,
    llm_calls,
    tokens_in,
    tokens_out,
    tokens_in + tokens_out AS total_tokens
FROM runs_v1
WHERE created_ms >= :from_ms
  AND created_ms < :to_ms
ORDER BY total_tokens DESC
LIMIT 100;
```

**English:** Rank runs by total recorded input and output tokens during a time
window. This is useful for finding unexpectedly expensive workloads before
opening any individual run.

**Why it is viable:** Token totals are resident run summaries. This query
does not scan prompts or responses and therefore needs neither CAS nor S3.

## 6. Which retained failures should I inspect?

```sql
SELECT
    call_id,
    run_id,
    definition_key,
    duration,
    status
FROM retained_calls_v1
WHERE run_id = :run_id
  AND status = 'errored'
ORDER BY duration DESC
LIMIT 100;
```

**English:** Within one run, list the slowest failed calls for which exact
instance evidence was retained.

**Why it is viable:** The run, status, duration, identity, and ordering fields
are resident metadata. Values are not hydrated merely to choose the calls.
The result is **retained failures**, not every failure; query 2 supplies the
population total.

## 7. What did one retained call receive and produce?

```sql
SELECT
    call_id,
    args,
    "return",
    error
FROM retained_calls_v1
WHERE run_id = :run_id
  AND call_id = :call_id;
```

**English:** Load the captured arguments, return value, and error for one
known retained call.

**Why it is viable:** Resident keys reduce the candidate set to one call
before value hydration. The canonical BAML value DAG, content-addressed pack
store, deduplication, and budgeted decoder already exist locally. Hosted v1
uses the same logical operation with authorized S3/CAS reads.

If a role was not captured, is pending, redacted, lost, truncated, corrupt, or
unsupported, that state must be reported explicitly; it must not silently
look like an ordinary SQL `NULL`.

## 8. Which retained calls contain a particular value?

```sql
SELECT
    call_id,
    run_id,
    definition_key
FROM retained_calls_v1
WHERE run_id = :run_id
  AND definition_key = :definition_key
  AND baml_value_int(
        baml_value_at_path(args, baml_path('arg[0].customer.age'))
      ) >= 30
LIMIT 100;
```

**English:** Within one run and one logical function, find up to 100 retained
calls whose first argument contains a customer age of at least 30.

**Why it is viable:** The engine first applies the resident run/function
filters, then batch-hydrates only distinct candidate values and evaluates the
typed path predicate. The final `LIMIT 100` applies after the value predicate;
it cannot be pushed below hydration and still be correct.

This is the right shape for targeted value investigation. An unbounded search
over every captured customer value is not a preferred v1 dashboard query and
there is deliberately no persistent plaintext, full-text, or scalar-value
index in ClickHouse.

## 9. Is the evidence complete enough to trust the answer?

```sql
SELECT
    kind,
    reason,
    SUM(count) AS affected_records
FROM capture_losses_v1
WHERE run_id = :run_id
GROUP BY kind, reason
ORDER BY affected_records DESC;
```

**English:** Summarize every recorded capture-loss or degradation reason for
the run before presenting an analytical answer as complete.

**Why it is viable:** Loss facts are small resident records, not inferred by
scanning values. Making every material loss path consistently queryable is a
C1 correctness gate; some current paths record counters or markers but do not
yet persist diagnostics consistently.

Every query also ends with an out-of-band `query_outcome`. Clients must check
it for snapshot identity, unavailable value evaluations, budget exhaustion,
cancellation, and completeness. Rows without that terminal outcome are not a
successfully completed answer.

## Query-writing rules of thumb

1. Start at the correct grain: `runs_v1` for runs,
   `cct_population_v1` for every-call totals, and `retained_calls_v1` for
   inspectable instances.
2. Filter on resident run, time, function, status, and ID columns before
   selecting or filtering `args`, `return`, or `error`.
3. Use population aggregates to find the problem, then hydrate a small set of
   retained calls to explain it.
4. Group across revisions with `definition_key`, not `function_id`.
5. Check availability and the terminal outcome before claiming that an answer
   is complete.
6. Expect the same public SQL semantics locally and hosted, but not the same
   physical plan. DataFusion owns semantics; local providers read canonical
   artifacts, and hosted providers push safe resident work to ClickHouse.

## Do not promise these in v1

- A retained-call count as the count of all calls.
- Raw ClickHouse SQL or physical table access.
- Persistent full-text, token, scalar/path, or vector indexes over customer
  value content.
- One query that transparently joins local and hosted data.
- Durable background query jobs for unbounded work.
- User-defined SQL/BAML functions or arbitrary query plugins.
- Silent treatment of unavailable values as `NULL` or “no match.”

## Design and implementation references

- [Canonical Project Studio ledger](CANONICAL/README.md)
- [Query semantics and logical catalog](CANONICAL/design/04-query-system.md)
- [Profiler and measured substrate](CANONICAL/design/03-profiler.md)
- [Local artifacts and CAS](CANONICAL/design/storage/local-artifacts.md)
- [Hosted ClickHouse boundary](CANONICAL/design/storage/clickhouse.md)
- [Delivery milestones Q1, Q2, and H2](CANONICAL/design/09-delivery-plan.md)
