# baml_query

`baml_query` is a local first SQL frontend for resident SQLite metadata with
transparent hydration of content-addressed values from files. DataFusion owns
SQL parsing, planning, joins, CTEs, residual predicates, limits, and UDF
execution.

## Storage contract

The SQLite `function_calls` table keeps resident fields plus nullable
`args_value_id`, `return_value_id`, and `error_value_id` columns. Each ID is a
32-byte SHA-256 digest stored as a SQLite `BLOB`; the corresponding local file
is `values/<lowercase-hex-id>.blob`. A blob is UTF-8 JSON and may contain a
reference such as `{"$value_ref":"<id>"}`.

The fixed-width digest is intentional: it has stable binary comparisons in
SQLite, avoids UUID formatting ambiguity, and verifies blob integrity on read.
`ValueId` is serialized as hex at API boundaries.

## Editing table schemas

Table definitions are intentionally code-owned rather than inferred from the
SQLite database. This keeps the SQL-facing schema stable while allowing the
physical tables to be owned by another system.

When an upstream table changes:

1. Update the `SqliteTableSpec::from_columns` definition passed to
   `with_table`.
2. Keep the logical name used by SQL stable when possible; change only the
   physical name when the upstream column was renamed.
3. Use `SqliteColumnSpec::new` for resident values and
   `SqliteColumnSpec::hydrated_value` for physical value-ID columns.
4. Set the Arrow `DataType` and nullability to match the logical contract.
5. Keep a logical `project_id` column on every registered table. Its physical
   name can be `tenant_key`, `workspace_id`, or another valid SQLite identifier.
6. Update relationship definitions if a join key or logical table changed.
7. Add or update an integration test using the real physical column names.

For new schemas, prefer `SqliteTableSpec` over
`FunctionCallsTableSpec`. `FunctionCallsTableSpec` is a compatibility helper
for the original fixed `function_calls` logical schema; it is not the general
schema-definition mechanism.

The mapping is explicit: the logical column name is what appears in SQL, while
the physical name is what SQLite reads. The engine does not inspect or migrate
the upstream schema automatically.

For the cloud handoff and the invariants the future ClickHouse/S3 service must
preserve, see [CLOUD_CLICKHOUSE_S3.md](CLOUD_CLICKHOUSE_S3.md).

For practical business and engineering/debugging queries, see
[EXAMPLE.md](EXAMPLE.md).

## Example

```rust,no_run
use std::sync::Arc;

use baml_query::{QueryEngine, SqliteFunctionCallsProvider};
use rusqlite::Connection;

# async fn example(connection: Connection) -> baml_query::Result<()> {
let provider = SqliteFunctionCallsProvider::from_connection(
    connection,
    "/tmp/baml-values",
    Arc::<str>::from("project-1"),
)?
.with_standard_resident_tables()?;
let engine = QueryEngine::new(provider)?;
let batches = engine
    .execute("SELECT id FROM function_calls WHERE contains(value_at(args, 0), 'hi') LIMIT 100")
    .await?;
# let _ = batches;
# Ok(())
# }
```

When the physical schema is owned by another system, define the logical schema
yourself and map each column to the external physical layout. The logical
names, Arrow types, ID representation, and hydration behavior are all caller
controlled:

```rust,no_run
use baml_query::{FunctionCallsTableSpec, SqliteFunctionCallsProvider};

# fn configure(mut provider: SqliteFunctionCallsProvider) -> baml_query::Result<SqliteFunctionCallsProvider> {
let calls = FunctionCallsTableSpec::new("trace_rows")?
    .with_column("id", "call_uuid")?
    .with_column("project_id", "tenant_key")?
    .with_column("name", "fn_name")?
    .with_column("args_value_id", "arg_cid")?;
let provider = provider.with_function_calls_table(calls);
# Ok(provider)
# }
```

Use `SqliteTableSpec::from_columns` and `SqliteColumnSpec` for arbitrary tables.
`SqliteColumnSpec::new` creates a resident column; use
`SqliteColumnSpec::hydrated_value` when the physical column contains a
content-addressed value ID:

```rust,no_run
use baml_query::{SqliteColumnSpec, SqliteTableSpec};
use datafusion::arrow::datatypes::DataType;

let events = SqliteTableSpec::from_columns(
    "events",
    "event_log",
    vec![
        SqliteColumnSpec::new("event_id", "event_key", DataType::UInt64, false),
        SqliteColumnSpec::new("project_id", "tenant_key", DataType::Utf8, false),
        SqliteColumnSpec::new("process_ref", "proc_uuid", DataType::Utf8, true),
        SqliteColumnSpec::hydrated_value("payload", "payload_cid", true),
    ],
)?;
let provider = provider.with_table(events)?;
```

`SqliteResidentTableSpec` remains as a compatibility name for
`SqliteTableSpec`; the default constructors still map each logical name to an
identically named physical column.

Declare joinability separately from physical mappings. Relationships use the
logical names and are validated against the registered table definitions:

```rust,no_run
use baml_query::{QueryEngine, RelationshipDefinition};

let engine = QueryEngine::new(provider)?
    .with_relationship(
        RelationshipDefinition::many_to_one(
            "events", "process_ref", "processes", "id",
        )
        .project_scoped(),
    )?;
```

This metadata supports composite keys and cardinality validation. It does not
change SQL syntax: queries still write explicit `JOIN ... ON ...` clauses.
Project-scoped relationships require a project column on both tables, while
each table provider independently applies the project filter.

Use `register_udf` or `register_async_udf` to add allowlisted DataFusion
functions. `with_budgets` controls candidate rows, recursive depth, expanded
bytes, blob bytes, distinct values, and query duration. `explain` returns the
DataFusion plan, including the lazy SQLite scan.

After `execute` or `explain`, call `engine.metrics()` for the last query's
candidate/output row counts, blob/cache counters, and phase timings. The
repeatable benchmark fixture can be run with:

```text
cargo bench -p baml_query --bench query_bench
```

For a local wall-clock report with phase metrics, use the explicit baseline
runner:

```text
BAML_QUERY_MANUAL_BENCH=1 cargo bench -p baml_query --bench query_bench
```

The provider intentionally does not push a final SQL `LIMIT` into SQLite when
hydrated predicates may still reject rows. Lazy batches let DataFusion stop
the scan once the final limit is satisfied.

DataFusion also executes aggregates over the streamed batches, including
`COUNT`, `COUNT(DISTINCT ...)`, `GROUP BY`, and `HAVING`. Resident filters are
still pushed into SQLite first, but aggregate pushdown is not implemented yet;
grouping therefore consumes the full candidate stream. Grouping hydrated JSON
values can be memory-intensive because the current logical representation is
serialized `LargeBinary` data.
