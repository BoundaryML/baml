# Query examples

This document shows the SQL currently supported by `baml_query`. The examples
use the default logical tables:

- `function_calls`: one row per function invocation.
- `threads`: one row per execution thread or logical worker thread.
- `processes`: one row per process, worker, or runtime instance.
- `ccts`: one row per call-context-tree node or execution context.

## Data model

The default schema is shaped like a small star schema: `function_calls` is the
large fact table, while `threads`, `processes`, and `ccts` are smaller metadata
tables that explain or group those calls.

### `function_calls`

This is the primary event table. Each row represents one function invocation
and is the table to start with when asking “what happened?” questions.

| Column | Meaning | Typical use |
| --- | --- | --- |
| `id` | Stable identifier for the invocation | Trace a single call or deduplicate results |
| `project_id` | Tenant/project scope | Isolate data and make portable joins safe |
| `process_id` | Process or worker that executed the call | Filter by runtime instance or join to `processes` |
| `thread_id` | Thread that executed the call | Inspect one thread or join to `threads` |
| `cct_id` | Call-context-tree node for the call | Group calls by execution context |
| `captured_ts` | Call capture timestamp as `Int64` | Time windows, timelines, and ordering |
| `name` | Function name | Find a function, compare functions, or group by function |
| `status` | Call outcome/status | Find failures, retries, and success rates |
| `metadata` | Resident metadata text | Display or filter lightweight call metadata |
| `metrics` | Resident metrics text | Inspect metrics stored with the call |
| `args` | Hydrated JSON arguments | Debug inputs and filter on argument fields |
| `return` | Hydrated JSON return value | Inspect outputs and application-level results |
| `error` | Hydrated JSON error value | Inspect structured failures |

The resident columns are cheap to scan compared with hydrated columns. A
query that selects or filters on `args`, `return`, or `error` may read local
value files. Start with resident predicates such as `name`, `status`, IDs, and
`captured_ts` before applying hydrated predicates.

`metadata` and `metrics` are currently exposed as resident text columns. They
may contain JSON in an upstream schema, but this engine does not automatically
treat them as hydrated `ValueId` columns. If they need structured hydration,
define separate hydrated columns in the table specification.

### `threads`

This table describes the thread or logical worker associated with a call. It
is usually much smaller than `function_calls` and is useful when users know a
human-readable thread name rather than a thread ID.

| Column | Meaning | Typical use |
| --- | --- | --- |
| `id` | Stable thread identifier | Join to `function_calls.thread_id` |
| `project_id` | Tenant/project scope | Keep thread lookups within the project |
| `process_id` | Owning process, when available | Join the thread to `processes` |
| `name` | Human-readable thread or worker name | Find calls from a named thread |

The default schema does not include `started_at`, `ended_at`, heartbeat, or
last-activity columns. If “was this thread running yesterday?” is a required
question, those fields must be added to the configured logical table schema or
inferred from another event/activity table.

### `processes`

This table describes the process, worker, or runtime instance that produced
calls. It is normally the smallest dimension table.

| Column | Meaning | Typical use |
| --- | --- | --- |
| `id` | Stable process identifier | Join to `function_calls.process_id` or `threads.process_id` |
| `project_id` | Tenant/project scope | Keep process lookups within the project |
| `name` | Process/program/worker name | Filter calls by the runtime that produced them |

Use this table for enrichment and human-readable filtering. Keep the stable
`process_id` directly on `function_calls` so queries that already know the ID
do not need a join.

### `ccts`

This table describes call-context-tree nodes. A CCT can represent a logical
execution context, stack-like grouping, or another hierarchy supplied by the
upstream system.

| Column | Meaning | Typical use |
| --- | --- | --- |
| `id` | Stable context-node identifier | Join to `function_calls.cct_id` |
| `project_id` | Tenant/project scope | Keep context lookups within the project |
| `thread_id` | Thread containing the context, when available | Join the context to `threads` |
| `name` | Human-readable context name | Group or filter calls by context |

The default `ccts` table is intentionally small and descriptive. If the
upstream CCT model has parent IDs, depths, source locations, or timestamps,
those can be added as resident columns through a custom table specification.

### Relationships

The usual logical relationships are:

```text
function_calls.process_id  → processes.id
function_calls.thread_id   → threads.id
function_calls.cct_id      → ccts.id
threads.process_id         → processes.id
ccts.thread_id             → threads.id
```

These are generally many-to-one relationships: many calls can belong to one
thread, process, or CCT. Every relationship is project-scoped in the default
model, so portable joins should include the project equality:

```sql
ON related.project_id = calls.project_id
AND related.id = calls.related_id
```

Use a regular join when you need columns from the related table. Use a
semi-join or `IN` pattern when the related table only determines which calls
are eligible. Because `function_calls` is expected to be much larger than the
dimension tables, avoid joining it to another large event table unless the
query is intentionally doing a broad event correlation.

### Choosing a starting table

| Question type | Start with | Why |
| --- | --- | --- |
| What calls happened? | `function_calls` | It contains the event rows and timestamps |
| Which functions are failing? | `function_calls` | `name` and `status` are resident |
| What did a call receive or return? | `function_calls` | `args`, `return`, and `error` are hydrated there |
| Which calls came from a named worker? | `threads` or `processes`, then filter calls | The dimension contains the readable name |
| How much work did each thread do? | `function_calls` grouped by `thread_id` | Avoids a join if only IDs are needed |
| Which execution context contains failures? | `function_calls` joined to `ccts` | CCT supplies the readable context name |
| Was a thread active during a time range? | `threads` plus lifecycle/activity columns | The default thread schema has no lifecycle timestamps |

The engine automatically scopes registered tables to the active project. The
examples still include `project_id` where it makes the tenant boundary clear
or where it is part of a join condition.

The logical schema is configurable. If the physical database uses names such
as `tenant_key`, `proc_uuid`, or `event_time`, map those physical columns to
the logical names before running these queries.

`captured_ts` is an `Int64`; replace the example values with the timestamp
units used by the configured table definition. `args`, `return`, and `error`
are hydrated JSON values exposed as `LargeBinary`. The value functions below
operate on those hydrated values.

## 1. Which calls failed recently?

**Question:** “Did the `send_email` function start failing, and how many calls
were affected during the incident window?”

```sql
SELECT
    name,
    status,
    COUNT(*) AS call_count
FROM function_calls
WHERE name = 'send_email'
  AND captured_ts >= 1720000000
  AND captured_ts < 1720003600
GROUP BY name, status
ORDER BY call_count DESC;
```

This is a resident-only aggregation. It does not hydrate blobs and can be
executed efficiently from SQLite-resident metadata.

## 2. Which functions are failing most often?

**Question:** “What are the top failing function names in the selected
project?”

```sql
SELECT
    name,
    COUNT(*) AS failures
FROM function_calls
WHERE status = 'error'
GROUP BY name
ORDER BY failures DESC
LIMIT 20;
```

This is useful for triage dashboards and regression detection. The provider’s
project filter keeps the result scoped to the active project.

## 3. Which calls belong to a particular process and thread?

**Question:** “Show every call for one thread while debugging a specific
process.”

```sql
SELECT
    id,
    captured_ts,
    name,
    status
FROM function_calls
WHERE thread_id = 'thread-42'
  AND process_id = 'process-7'
ORDER BY captured_ts;
```

This is the preferred shape when the caller already knows the stable IDs. It
avoids a join entirely and lets the large `function_calls` table filter on its
own resident keys.

## 4. Which calls happened in a named thread?

**Question:** “Find calls in threads named `checkout-worker`, including only
threads associated with the expected process.”

```sql
SELECT
    fc.id,
    fc.captured_ts,
    fc.name,
    fc.status
FROM function_calls AS fc
JOIN threads AS t
  ON t.project_id = fc.project_id
 AND t.id = fc.thread_id
WHERE t.name = 'checkout-worker'
  AND fc.process_id = 'process-7'
ORDER BY fc.captured_ts;
```

This is a fact-to-dimension join: the large table is `function_calls`, while
`threads` is expected to be much smaller. The explicit project equality makes
the tenant boundary visible in the query.

## 5. Which calls ran in a process with a particular program name?

**Question:** “Find errors produced by calls running in the `worker` process.”

```sql
SELECT
    fc.id,
    fc.name,
    fc.status,
    fc.captured_ts
FROM function_calls AS fc
JOIN processes AS p
  ON p.project_id = fc.project_id
 AND p.id = fc.process_id
WHERE p.name = 'worker'
  AND fc.status = 'error'
ORDER BY fc.captured_ts DESC;
```

The process table supplies a human-readable filter, while the result remains
call-level data. The same logical query can later be implemented as a
ClickHouse join, semi-join, dictionary lookup, or materialized view.

## 6. Filter calls by a small set of matching threads

**Question:** “Which calls happened in threads that were named `checkout` or
`payments`?”

```sql
SELECT
    fc.id,
    fc.thread_id,
    fc.name,
    fc.captured_ts
FROM function_calls AS fc
WHERE fc.project_id = 'project-1'
  AND fc.thread_id IN (
    SELECT t.id
    FROM threads AS t
    WHERE t.project_id = 'project-1'
      AND t.name IN ('checkout', 'payments')
)
ORDER BY fc.captured_ts;
```

This is a semi-join: the subquery produces eligible thread IDs, but no thread
columns are returned. It also avoids duplicating calls if the related table
contains multiple matching records.

## 7. What calls were active during a time window?

**Question:** “Show the call timeline for yesterday’s incident window.”

```sql
SELECT
    captured_ts,
    name,
    status,
    thread_id,
    process_id
FROM function_calls
WHERE captured_ts >= 1720000000
  AND captured_ts < 1720086400
ORDER BY captured_ts, id;
```

This filters on the event timestamp stored on the fact table. If “running
yesterday” means that a thread’s lifetime overlapped yesterday, use the
thread’s start/end columns in a relationship filter instead; that requires
those columns to be included in the configured `threads` schema.

## 8. Which argument field was sent to a function?

**Question:** “For calls with an argument object, what subject was sent to
the function?”

```sql
SELECT
    id,
    name,
    value_string(value_field(args, 'subject')) AS subject
FROM function_calls
WHERE name = 'send_email'
  AND value_field(args, 'subject') IS NOT NULL
ORDER BY captured_ts DESC;
```

`value_field` reads a field from hydrated JSON and `value_string` converts the
field to a SQL string. Because this uses `args`, matching rows may require blob
reads after resident filters have been applied.

## 9. Find calls whose first argument contains a value

**Question:** “Which calls passed `customer-123` anywhere in their first
argument array?”

```sql
SELECT
    id,
    name,
    captured_ts
FROM function_calls
WHERE contains(value_at(args, 0), 'customer-123')
ORDER BY captured_ts DESC;
```

This assumes `args` is an array and that its first element is itself a value
that `contains` can inspect. The exact JSON shape matters; malformed or
missing value objects produce hydration errors rather than silently matching.

## 10. Inspect a nested return value

**Question:** “Which calls returned a response with `status = 'rejected'`?”

```sql
SELECT
    id,
    name,
    value_string(value_field("return", 'status')) AS return_status
FROM function_calls
WHERE value_string(value_field("return", 'status')) = 'rejected'
ORDER BY captured_ts DESC;
```

This is useful for debugging application-level failures that were recorded in
the returned value rather than in the resident `status` column.

## 11. How many calls did each process make?

**Question:** “Which processes generated the most calls in the selected
project?”

```sql
SELECT
    process_id,
    COUNT(*) AS call_count,
    COUNT(DISTINCT thread_id) AS thread_count
FROM function_calls
GROUP BY process_id
ORDER BY call_count DESC;
```

This aggregation stays entirely on resident columns. It is a useful baseline
for comparing local SQLite/DataFusion performance with a future ClickHouse
implementation.

## 12. Which function names are used by each thread?

**Question:** “What work did each thread perform during a debugging session?”

```sql
SELECT
    thread_id,
    name,
    COUNT(*) AS call_count,
    MIN(captured_ts) AS first_call_ts,
    MAX(captured_ts) AS last_call_ts
FROM function_calls
WHERE captured_ts >= 1720000000
  AND captured_ts < 1720086400
GROUP BY thread_id, name
ORDER BY thread_id, call_count DESC;
```

This produces a compact activity summary without reading any hydrated values.

## 13. Which call-context trees contain errors?

**Question:** “Which call-context-tree nodes contain failed calls, and what
are their names?”

```sql
SELECT
    c.id AS cct_id,
    c.name AS cct_name,
    COUNT(*) AS failed_calls
FROM function_calls AS fc
JOIN ccts AS c
  ON c.project_id = fc.project_id
 AND c.id = fc.cct_id
WHERE fc.status = 'error'
GROUP BY c.id, c.name
ORDER BY failed_calls DESC;
```

This is another small-dimension-to-large-fact join. It is appropriate when
the result needs dimension attributes such as the CCT name.

## 14. Which argument categories have the highest failure rate?

**Question:** “Is one input category causing a disproportionate number of
failed calls?”

```sql
SELECT
    value_string(value_field(args, 'category')) AS category,
    COUNT(*) AS total_calls,
    SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) AS failed_calls
FROM function_calls
WHERE value_field(args, 'category') IS NOT NULL
GROUP BY value_string(value_field(args, 'category'))
HAVING COUNT(*) >= 10
ORDER BY failed_calls DESC, total_calls DESC;
```

This combines resident aggregation with hydrated grouping. It is expressive,
but can be memory-intensive locally because DataFusion groups serialized JSON
values after hydration. It is a strong candidate for a future ClickHouse
materialized view if it becomes a frequent dashboard query.

## 15. Find calls with missing or present hydrated values

**Question:** “Which calls have no recorded arguments, returns, or errors?”

```sql
SELECT
    id,
    name,
    status,
    args,
    "return",
    error
FROM function_calls
WHERE args IS NULL
   OR "return" IS NULL
   OR error IS NOT NULL
ORDER BY captured_ts DESC;
```

This query checks whether the hydrated logical columns are present. Selecting
the hydrated columns causes value loading; selecting only resident columns
such as `id`, `name`, and `status` avoids blob reads.

## Query design notes

- Use direct `function_calls` predicates when the caller already knows IDs.
- Use joins when related columns are needed in the result.
- Use `IN`/semi-join patterns when a small dimension table only filters the
  large fact table.
- Keep `project_id` in relationship predicates when writing portable SQL,
  even though the provider also applies project scoping automatically.
- Apply resident filters before asking for hydrated values whenever possible.
- Use `EXPLAIN` and `engine.metrics()` to see the plan, candidate rows,
  hydration work, and serialization cost.
