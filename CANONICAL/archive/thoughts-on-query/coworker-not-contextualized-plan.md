Here's the full plan, adapted for a local SQLite/database + local blob-file prototype.

## 1. Goal

Build a Rust crate that accepts SQL like:

```sql
SELECT id
FROM function_calls
WHERE project_id = 'p1'
  AND name = 'send_email'
  AND contains(value_at(args, 0), 'hi')
  AND root.classify(value_at(args, 0)) = 'negative'
LIMIT 100;
```

The user sees logical columns:

```text
args
return
error
```

But physically:

```text
args_value_id
return_value_id
error_value_id
```

refer to separately stored value files.

## 2. Recommended crate

Create:

```text
crates/baml_query/
```

Keep it initially independent from the BAML runtime.

Use:

- Apache DataFusion for SQL parsing, logical planning, physical execution, and Arrow batches
- `rusqlite` for the local SQLite database
- local filesystem blobs initially
- async Rust for hydration and UDF execution
- a trait-based design so SQLite can later become ClickHouse and local files can later become S3

## 3. Architecture

```text
SQL
 │
 ▼
Query validation
 │
 ▼
DataFusion logical plan
 │
 ▼
SqliteTableProvider
 ├── Push resident predicates into SQLite
 ├── Select hidden value IDs
 └── Produce Arrow batches
 │
 ▼
HydrateExec
 ├── Read value files
 ├── Follow nested references
 ├── Deduplicate and cache
 └── Add args / return / error
 │
 ▼
Residual DataFusion filters
 ├── contains(...)
 └── root.classify(...)
 │
 ▼
Projection / final LIMIT
```

SQLite should only execute the part of the query it can evaluate exactly.

## 4. Physical SQLite schema

Start with one table:

```sql
CREATE TABLE function_calls (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    process_id TEXT,
    thread_id TEXT,
    cct_id TEXT,
    captured_ts INTEGER,

    name TEXT NOT NULL,
    status TEXT,
    metadata TEXT,
    metrics TEXT,

    args_value_id BLOB,
    return_value_id BLOB,
    error_value_id BLOB
);

CREATE INDEX function_calls_project_name
    ON function_calls(project_id, name);

CREATE INDEX function_calls_captured_ts
    ON function_calls(project_id, captured_ts);
```

### ID representation

Use a Rust newtype:

```rust
pub struct ValueId([u8; 32]);
```

Generate IDs using a content hash such as BLAKE3.

SQLite stores the IDs as nullable 32-byte `BLOB`s:

```text
args_value_id       = 32-byte identifier
values/<hex-id>.blob = actual serialized value
```

This gives deduplication and shared subvalues naturally.

Use UUIDs or text IDs for ordinary entities such as `function_calls.id`; use content IDs for hydrated values.

## 5. Value storage

Define:

```rust
#[async_trait]
pub trait ValueStore: Send + Sync {
    async fn get_many(
        &self,
        ids: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, Bytes>, QueryError>;
}
```

Initial implementation:

```text
LocalBlobStore
└── root directory
    └── values/
        ├── <hex-id>.blob
        └── ...
```

The store must:

- deduplicate requested IDs
- use a per-query cache
- read files concurrently with a limit
- support cancellation
- enforce byte/request limits
- report missing or corrupt values clearly

Later implementations can support S3, packed segments, or range reads without changing the query engine.

## 6. Hydration

Define:

```rust
#[async_trait]
pub trait ValueHydrator: Send + Sync {
    async fn hydrate_many(
        &self,
        roots: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, HydratedValue>, QueryError>;
}
```

The hydrator owns:

- recursive child-reference traversal
- transitive deduplication
- cycle detection
- maximum depth
- maximum expanded size
- decoding
- reconstruction of arrays, objects, arguments, returns, and errors

For every Arrow batch, `HydrateExec` should:

1. Read the required hidden ID columns.
2. Deduplicate IDs.
3. Check the query cache.
4. Fetch missing blobs.
5. Hydrate recursively.
6. Reconstruct results in original row order.
7. Remove hidden ID columns.
8. Add logical hydrated columns.

If a value ID is `NULL`, return SQL `NULL` without reading a file.

## 7. Value representation

Avoid a complex recursive Arrow type initially.

Represent hydrated values as:

```text
Arrow physical type: LargeBinary
Logical type: BamlValue
```

Attach Arrow extension metadata identifying the logical type.

The bytes contain a canonical serialized representation. The first codec can be JSON-based for development, provided it has:

- a version
- deterministic serialization
- explicit reference encoding
- clear type/error semantics

A more compact binary codec can be added later.

## 8. DataFusion table provider

Implement:

```rust
pub struct SqliteTableProvider {
    // SQLite connection/configuration
    // logical schema
    // physical-to-logical column mapping
}
```

The logical schema contains:

```text
id
project_id
name
status
metadata
metrics
args
return
error
```

The physical mapping knows:

```text
args   → args_value_id
return → return_value_id
error  → error_value_id
```

The provider should:

1. Identify projected resident columns.
2. Identify hydrated columns required by projections or filters.
3. Add hidden value-ID columns to the SQLite query.
4. Classify filters.
5. Generate parameterized SQLite SQL.
6. Create a SQLite scan execution plan.
7. Wrap the scan with `HydrateExec` when needed.
8. Remove hidden columns before output.

## 9. Filter pushdown

Push into SQLite:

- resident-column comparisons
- `=`, `!=`, `<`, `<=`, `>`, `>=`
- `IS NULL`
- `IS NOT NULL`
- `IN` with literals
- `AND` where every branch is pushable
- mandatory project/tenant predicates

Keep local:

- anything involving `args`, `return`, or `error`
- `value_at`
- `contains`
- custom UDFs
- unknown functions
- mixed resident/hydrated expressions
- unsafe `OR` expressions

For example:

```sql
name = 'send_email'
AND contains(value_at(args, 0), 'hi')
```

becomes:

```text
SQLite:
    name = 'send_email'

DataFusion:
    contains(value_at(args, 0), 'hi')
```

Do not partially push:

```sql
name = 'send_email'
OR contains(value_at(args, 0), 'hi')
```

unless correctness can be proven.

## 10. LIMIT behavior

Never push `LIMIT 100` into SQLite if a later hydrated predicate may reject rows.

Correct behavior:

```text
Read SQLite candidates
→ hydrate
→ apply local filters
→ continue until 100 matches
```

This is essential for queries where the first 500 candidates fail the hydrated predicate.

Only push the limit when all preceding filters are executed exactly in SQLite.

## 11. Value functions

Initially register:

```sql
value_at(args, 0)
value_field(value, 'field_name')
value_string(value)
contains(value, 'text')
```

Semantics:

- string + string → substring search
- `NULL` → SQL `NULL`
- incompatible type → SQL `NULL` or a clear query error, depending on function
- invalid value encoding → query failure
- array index out of bounds → `NULL`

Defer method syntax such as:

```sql
args[0].contains('hi')
```

until the basic expression system works.

## 12. UDF system

Define a registry of allowlisted functions:

```rust
pub struct FunctionDescriptor {
    pub sql_name: String,
    pub version: String,
    pub deterministic: bool,
    pub cacheable: bool,
    pub batchable: bool,
    pub cost: FunctionCost,
}
```

Functions should execute in batches rather than once per SQL row.

Example:

```sql
root.classify(value_at(args, 0))
```

The execution path should:

1. Decode input values.
2. Deduplicate equal inputs.
3. Check the function-result cache.
4. Batch uncached inputs.
5. Execute with bounded concurrency.
6. Restore output order.
7. Return an Arrow array.

Cache keys should include:

```text
function name
function version
configuration/model version
canonical input hash
```

Only registered functions may run. No arbitrary user code execution in version one.

## 13. Query context and budgets

Every query gets:

```rust
pub struct QueryContext {
    pub query_id: Uuid,
    pub project_id: String,
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub budgets: QueryBudgets,
}
```

Budgets:

```text
maximum SQLite candidate rows
maximum distinct value IDs
maximum blob requests
maximum blob bytes
maximum decoded bytes
maximum hydration depth
maximum UDF invocations
maximum UDF concurrency
maximum wall time
```

The project restriction should be injected by the engine/provider, not trusted to user SQL.

Reject:

- non-SELECT statements
- multiple SQL statements
- unknown tables
- unknown UDFs
- cross-project access
- plans exceeding configured budgets

## 14. Suggested module layout

```text
src/
├── lib.rs
├── query_engine.rs
├── query_context.rs
├── error.rs
├── sqlite/
│   ├── mod.rs
│   ├── provider.rs
│   ├── scan_exec.rs
│   └── pushdown.rs
├── values/
│   ├── mod.rs
│   ├── value_id.rs
│   ├── value_store.rs
│   ├── local_blob_store.rs
│   ├── decoder.rs
│   ├── hydrator.rs
│   └── hydrate_exec.rs
├── expressions/
│   ├── mod.rs
│   ├── value_at.rs
│   ├── value_field.rs
│   └── contains.rs
├── udfs/
│   ├── mod.rs
│   ├── registry.rs
│   ├── descriptor.rs
│   └── executor.rs
└── tests/
```

## 15. Milestones

### Milestone 1: resident-only queries

Support:

```sql
SELECT id, name, status
FROM function_calls
WHERE project_id = 'p1'
  AND name = 'foo'
LIMIT 100;
```

Implement:

- DataFusion context
- SQLite provider
- Arrow batch conversion
- projection/filter pushdown
- read-only SQL validation
- `EXPLAIN`

### Milestone 2: hydration

Support:

```sql
SELECT id, args
FROM function_calls
WHERE name = 'foo';
```

Implement:

- `ValueId`
- local blob store
- decoder
- recursive hydrator
- caching
- `HydrateExec`

### Milestone 3: value expressions

Support:

```sql
SELECT id
FROM function_calls
WHERE contains(value_at(args, 0), 'hi')
LIMIT 100;
```

Implement:

- `BamlValue`
- `value_at`
- `value_field`
- `contains`
- null/type behavior
- limit correctness

### Milestone 4: UDFs

Support:

```sql
SELECT id
FROM function_calls
WHERE root.classify(value_at(args, 0)) = 'negative';
```

Implement:

- function registry
- batch execution
- result cache
- bounded concurrency
- cancellation
- fake deterministic classifier

### Milestone 5: predicate staging

Order filters by location and cost:

```text
SQLite resident filters
→ cheap hydrated filters
→ expensive UDF filters
→ final limit
```

Add plan snapshots and `EXPLAIN` output.

### Milestone 6: hardening

Add:

- query cancellation tests
- blob corruption tests
- tenant isolation tests
- fuzz tests for decoding
- load tests
- metrics
- CTE/nested-query tests
- eventual S3 backend

## 16. Required tests

At minimum:

1. Resident-only query performs zero blob reads.
2. Selecting only `return` does not hydrate `args`.
3. Repeated IDs are fetched once per query.
4. Shared nested child IDs are fetched once.
5. `NULL` IDs do not trigger reads.
6. Missing values produce useful errors.
7. The final `LIMIT` returns actual matches after hydration.
8. UDFs execute in batches.
9. UDF results are cached correctly by version/configuration.
10. CTEs work with outer hydrated filters.
11. Unsafe `OR` predicates are not incorrectly pushed down.
12. Cancellation stops SQLite, hydration, and UDF work.
13. User SQL cannot bypass project filtering.

The first useful end-to-end target is therefore:

```sql
SELECT id
FROM function_calls
WHERE project_id = 'p1'
  AND name = 'send_email'
  AND contains(value_at(args, 0), 'hi')
LIMIT 100;
```

Once that works correctly, the rest of the system has a solid foundation.