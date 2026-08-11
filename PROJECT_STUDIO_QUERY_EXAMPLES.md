# Project Studio query examples

This is the short, shareable introduction to the questions Project Studio is
designed to answer. It follows the shape of the local query-engine
[example](https://github.com/BoundaryML/baml/blob/codex/local-query-engine/baml_language/crates/baml_query/EXAMPLE.md),
but uses the canonical Project Studio model rather than the prototype's
all-call `function_calls` table.

> **Status:** these are target v1 SQL queries, not commands that work on this
> branch today. The row meanings and relationships are settled. Exact column
> types, lifecycle status spellings, parameter binding, and BAML value-function
> names freeze in milestone Q1. This guide intentionally leaves version
> suffixes off table names for readability. In the examples, `:name` denotes a
> bound parameter.

## Data model

Project Studio does not store every call as an individual database row. It
keeps complete summary counters for all calls, then keeps individual calls and
their values only when capture policy says to retain them. That distinction is
the key to reading every query below.

### Queryable tables

| Table | One row represents | What it is for |
| --- | --- | --- |
| `runs` | One program run | Lifecycle state, revision, total calls/errors, LLM/token totals, and evidence health |
| `cct_population` | One function at one location in a run's nested call tree | Complete counts and timing summaries across all calls |
| `cct_windows` | One recent time window for one call-tree location | Live/recent changes; these rows are not added to final totals |
| `llm_population` | One run, call-tree location, and model combination | Aggregated LLM calls, tokens, and provider/parser errors |
| `retained_calls` | One individual call that capture policy kept | Find exact calls and, when authorized and available, load arguments, returns, or errors |
| `spawn_edges` | One summarized parent-to-child task relationship | Counts of spawned, completed, failed, cancelled, running, and waiting work |
| `spawn_instances` | One individually retained spawned task | Inspect a specific child task when exact evidence was kept |
| `exact_windows` | One retained window of detailed events | Explain an incident using nearby events and show whether the window was complete |
| `capture_losses` | One recorded loss or degradation fact | Explain why evidence is incomplete without guessing from missing values |
| `functions` | One function in one compiled revision | Function name, source location, stable cross-revision identity, and definition hash |
| `revisions` | One compiled program revision | Connect runs and functions to the source snapshot and compiler inputs that produced them |
| `observations_active` | One retained operation that is still running | Show durable in-progress work and which fields are still pending |
| `observations_terminal` | One retained operation that has ended | Show completed, failed, cancelled, or abandoned work |

The `cct_population` name is historical; read it as “call-tree summaries.”
In the SQL, `fqn` is the full function name and `definition_key` is the stable
identity used to recognize the same logical function across revisions.

The `llm_population` shape is designed around today's LLM functions. Aaron's
changes are expected to change this part of the model, so its final rows and
columns should be revisited with that work rather than treated as frozen.

### What is stored outside these tables

- Local exact evidence and captured values live in the project's `.baml`
  files and deduplicated value store.
- Hosted exact evidence, arguments, returns, errors, and log bodies live in
  S3. S3 is the durable source of those bytes, not a public SQL table.
- Hosted ClickHouse contains the small fields needed to filter, join, count,
  group, and order results. It may contain internal authorized references and
  reasons a value is unavailable, but not customer prompt, response, error, or
  log text.
- PostgreSQL contains ownership, upload, projection, audit, retention, and
  deletion workflow state. It is not the analytical event database.
- Selecting `args`, `return`, or `error` from `retained_calls` loads the value
  on demand from the local value files or S3 after cheaper table filters have
  narrowed the candidate calls.

There is deliberately no SQL table containing every individual call, and no
standing table of decoded customer-value fields or searchable value text.

## The three query shapes

| Shape | Use it for | Why it can be fast | Readiness |
| --- | --- | --- | --- |
| Summary queries | Counts, failures, time, tokens, and hot call locations across every call | The runtime continuously maintains small per-run and per-call-location totals instead of writing one analytical row per call | The profiling and summary-file foundation is built and measured; the public SQL reader is Q1/Q2 work |
| Individual retained calls | Finding specific calls that capture policy kept | Filters use small fields such as IDs, status, function identity, and time before any large value is read | The logical model is settled; production local and hosted readers are Q2/H2 work |
| Arguments, returns, and errors | Inspecting or filtering the values of retained calls | The query first narrows candidates, avoids loading duplicate values, and reads value bytes in bounded batches | The canonical value format/store exists and the prototype demonstrates on-demand loading; production value readers are Q2/H2 work |

Complete summaries and retained-call answers are intentionally different. A
row in `cct_population` contributes to totals across every call. A row in
`retained_calls` exists only when an individual execution was retained.

### What backs the viability claims

- The current profiler has reported 74.4 ns end-to-end overhead per call,
  47.8–48.6 ns for its call-tree bookkeeping, a 4.5 KiB result after five million
  calls, 2.62 ms completed-run first-frame open, and 34.3 MiB consumer RSS
  under sustained load on one development machine. These are architecture
  evidence and regression seeds, not performance promises for every machine.
- The canonical BAML value format, deduplicated value files, cleanup, and
  bounded reads are implemented locally.
- The linked query-engine prototype demonstrates pushing cheap filters into
  storage, loading values only when needed, evaluating remaining filters in
  the query layer, applying the final limit in the correct place, cancellation,
  and query-wide budgets. Its physical `function_calls`/SQLite/JSON-blob model
  is not the production contract.
- Production local SQL and the hosted ClickHouse/S3 path are designed and
  gated in Q1, Q2, and H2; they are not implemented on this branch.

## 1. Which recent runs had problems?

**English:** List the newest runs that recorded at least one error or whose
evidence was marked degraded. This is the first query to use for incident
triage.

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

**Why it is viable:** `runs` has one small row per run. The query reads no
customer values and can perform its filter, ordering, and limit directly in a
local table or hosted ClickHouse.

## 2. Which functions fail most often?

**English:** Across every call in the selected time window, rank functions by
how many executions ended in error and show their failure rate.

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

**Why it is viable:** The runtime already maintains complete call and error
totals. The query does not need one row per invocation or any argument/return
bytes. `definition_key` identifies the same logical function across revisions,
unlike `function_id`, which is local to one revision.

## 3. Where is execution time being spent?

**English:** For one run, show the functions responsible for the most direct
work, along with inclusive and waiting time. This identifies where to begin a
performance investigation.

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

**Why it is viable:** The profiler updates these counters as calls run and
writes small summary rows. No individually retained call or captured argument
is required. The mean is a directional summary, not a percentile or proof of
a regression.

## 4. Did a function change across revisions?

**English:** Compare call volume, failures, and average direct execution time
for the same logical function across compiled revisions.

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

**Why it is viable:** The system deliberately gives the same logical function
a stable identity across revisions, and these metrics are already complete
summary fields. Treat the result as a signal to investigate; a statistically
credible regression claim needs distribution and sample checks beyond this
summary.

## 5. Which runs used the most LLM tokens?

**English:** Rank runs by total recorded input and output tokens during a time
window. This is useful for finding unexpectedly expensive workloads before
opening any individual run.

> **LLM model note:** this query is designed around the current LLM functions.
> It is expected to change with Aaron's LLM work, so treat the fields and row
> model here as a useful starting point, not a frozen contract.

```sql
SELECT
    run_id,
    created_ms,
    revision_id,
    llm_calls,
    tokens_in,
    tokens_out,
    tokens_in + tokens_out AS total_tokens
FROM runs
WHERE created_ms >= :from_ms
  AND created_ms < :to_ms
ORDER BY total_tokens DESC
LIMIT 100;
```

**Why it is viable:** Token totals are small run-summary fields. This query
does not scan prompts or responses and therefore does not read value files or
S3 objects.

## 6. Which retained failures should I inspect?

**English:** Within one run, list the slowest failed calls for which exact
instance evidence was retained.

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

**Why it is viable:** The run, status, duration, identity, and ordering fields
are small fields stored directly in the table. Values are not loaded merely to
choose the calls. The result is **retained failures**, not every failure;
query 2 supplies the complete total.

## 7. What did one retained call receive and produce?

**English:** Load the captured arguments, return value, and error for one
known retained call.

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

**Why it is viable:** The run and call IDs reduce the candidate set to one call
before any large value is loaded. BAML's deduplicated value format, value
files, and bounded reader already exist locally. Hosted v1 uses the same
logical operation with authorized S3 reads.

If a role was not captured, is pending, redacted, lost, truncated, corrupt, or
unsupported, that state must be reported explicitly; it must not silently
look like an ordinary SQL `NULL`.

## 8. Which retained calls contain a particular value?

**English:** Within one run and one logical function, find up to 100 retained
calls whose first argument contains a customer age of at least 30.

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

**Why it is viable:** The engine first applies the cheap run/function filters,
then loads each distinct candidate value in bounded batches and checks the
requested field. The final `LIMIT 100` applies after the value condition; it
cannot be applied before the values are checked and still be correct.

This is the right shape for targeted value investigation. An unbounded search
over every captured customer value is not a preferred v1 dashboard query and
there is deliberately no persistent plaintext, full-text, or scalar-value
index in ClickHouse.

## 9. Is the evidence complete enough to trust the answer?

**English:** Summarize every recorded capture-loss or degradation reason for
the run before presenting an analytical answer as complete.

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

**Why it is viable:** Loss facts are small records stored directly in the
table; the query does not infer them by scanning values. Making every material
loss path consistently queryable is a C1 correctness gate; some current paths
record counters or markers but do not yet persist diagnostics consistently.

Every query also sends a separate final completion record called
`query_outcome`. Clients must check which data snapshot was used, whether any
needed values were unavailable, and whether the query hit a budget or was
cancelled. Rows without that final record are not a successfully completed
answer.

## Query-writing rules of thumb

1. Start with the table whose rows match the question: `runs` for runs,
   `cct_population` for every-call totals, and `retained_calls` for inspectable
   individual calls.
2. Filter on run, time, function, status, and ID columns before
   selecting or filtering `args`, `return`, or `error`.
3. Use complete summaries to find the problem, then load values from a small
   set of retained calls to explain it.
4. Group across revisions with `definition_key`, not `function_id`.
5. Check availability and the terminal outcome before claiming that an answer
   is complete.
6. Expect the same public SQL semantics locally and hosted, but not the same
   storage plan. The query layer defines what the SQL means; local readers use
   `.baml` files, while hosted readers send safe table work to ClickHouse and
   load values from S3 only when required.

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
- [Local artifacts and value store](CANONICAL/design/storage/local-artifacts.md)
- [Hosted ClickHouse boundary](CANONICAL/design/storage/clickhouse.md)
- [Delivery milestones Q1, Q2, and H2](CANONICAL/design/09-delivery-plan.md)
