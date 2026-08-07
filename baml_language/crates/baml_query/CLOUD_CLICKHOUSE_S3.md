# Cloud ClickHouse + S3 handoff

This document records the decisions the local implementation makes so a
future closed-source cloud implementation can be built independently. The
cloud service does not need to reuse DataFusion or the SQLite provider.

## Preserve the logical contract

The cloud implementation should preserve these user-visible concepts:

- Logical table and column names are separate from physical storage names.
- Every queryable table has a project/tenant scope exposed logically as
  `project_id`.
- Relationship definitions use logical table and column names.
- Resident columns are directly queryable values.
- Hydrated columns are content-addressed values resolved through a value store.
- `ValueId` values are 32-byte SHA-256 content IDs, represented as hex at API
  boundaries and as binary values in storage.
- Hydration may contain nested value references and must enforce depth, byte,
  distinct-value, timeout, and cancellation budgets.
- Portable functions such as `value_at`, `value_field`, `value_string`, and
  `contains` should retain their logical behavior.

The local implementation is the reference for semantics, not for physical
execution. A cloud result should match the local result for the portable query
corpus, including NULL handling, filtering, aggregation, and join cardinality.

## Recommended ClickHouse shape

Use a fact/dimension layout first:

```text
function_calls  -- very large fact table
threads         -- small dimension table
processes       -- small dimension table
```

`function_calls` should contain the keys needed for common filters directly:

```text
project_id
thread_id
process_id
name
captured_ts
call_id
args_value_id
return_value_id
error_value_id
```

This makes the common query join-free:

```sql
SELECT *
FROM function_calls
WHERE project_id = ?
  AND thread_id = ?
  AND process_id = ?
  AND name = ?
```

Filtering calls by a small set of matching threads or processes can use a
semi-join/`IN` shape. Enriching calls with thread or process attributes can use
a regular join. The cloud planner may choose a ClickHouse join, semi-join,
dictionary lookup, projection, or materialized view. Do not copy every large
or mutable dimension field into every call row without a measured reason.

Partitioning and sort keys must be chosen from measured query patterns. At a
minimum, benchmark project, time-range, thread, process, and function-name
filters against realistic data sizes. Do not treat the local SQLite indexes as
an automatic ClickHouse schema prescription.

## S3 value storage

The local value store uses this logical layout:

```text
values/<lowercase-hex-value-id>.blob
```

The object body is UTF-8 JSON. A JSON value may contain a reference such as:

```json
{"$value_ref":"<value-id>"}
```

The cloud implementation may use S3 directly, a cached object service, or a
ClickHouse-side value table, but it should preserve:

- deterministic content-addressed object keys;
- SHA-256 integrity verification;
- missing-object and corrupt-object errors;
- bounded recursive expansion;
- bounded total downloaded/expanded bytes;
- per-query deduplication and caching;
- cancellation and request deadlines;
- tenant authorization before object access.

Hydration should normally happen in the cloud query service or a controlled
value service, not by granting ClickHouse arbitrary S3 access to every object.
The service should batch value requests and avoid downloading hydrated columns
when the query projection does not use them.

## Query/API boundary

The default cloud API should accept the same logical SQL and parameters as the
local engine for the portable subset. The service should parse, authorize,
validate, and plan the request before issuing ClickHouse SQL. It must not trust
`project_id` supplied by an untrusted caller as the sole authorization check;
the tenant should come from authenticated request context and be applied to
every table and value-store request.

ClickHouse-specific features may be added as explicit cloud-only extensions:

- dictionaries or direct key-value lookups;
- `ANY`, `SEMI`, `ANTI`, or `ASOF` joins;
- projections and materialized views;
- ClickHouse-specific JSON, array, and date functions;
- backend-specific settings and `EXPLAIN` output.

Those features should be reported through capabilities or a clear
cloud-only-query error. They should not silently change the meaning of a
portable query.

## Required cloud validation

Before calling the cloud implementation production-ready, run a shared query
corpus against local fixtures and representative ClickHouse data. Include:

- project isolation across every table and hydrated object;
- fact-to-dimension joins and semi-joins;
- duplicate-key and NULL join behavior;
- time-range filtering for “running yesterday” semantics;
- resident and hydrated predicates;
- `COUNT`, `COUNT(DISTINCT ...)`, `GROUP BY`, and `HAVING`;
- missing and corrupt value objects;
- query cancellation, timeouts, and memory/row budgets;
- large fact-table performance and realistic concurrency.

The local benchmark and metrics names are intended to provide the vocabulary
for comparing SQLite, DataFusion, ClickHouse, and S3 phases. The numbers will
not be directly comparable across backends, but the query shapes and outcome
correctness should be.
