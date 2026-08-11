# Storage index

The shortest rule is: exact evidence lives in local artifacts or accepted S3 objects; transaction/workflow truth lives in PostgreSQL; hosted non-value analytics live in ClickHouse; queues and local analytical indexes are replaceable.

| Store | Contains | Must not contain | Authority |
|---|---|---|---|
| Local **.baml** | Sessions, boundary history, dictionaries, exact windows, value records, canonical CAS | Hosted tenancy/control truth | Canonical local evidence |
| Local provider cache | SQLite/Parquet/direct indexes selected per logical relation | Irreplaceable evidence | Rebuildable |
| Local **control.sqlite** *(target; not implemented)* | Spool ownership, receipts/watermarks, policy, pending operations, local attachments | Event/value analytical facts | Non-rebuildable local control |
| S3 accepted-evidence objects | Accepted source-range chunks, canonical CAS-bearing artifacts, anchored receipts, accepted source/schema snapshots | Mutable workflow rows | Canonical hosted evidence |
| S3 derived/temporary objects | Immutable projector snapshots, rebuildable manifest acceleration, temporary scan/export/query results | Sole copy of accepted evidence | Rebuildable or ephemeral, policy-scoped |
| PostgreSQL | Identity, routing, authz, commitment, idempotency, outbox, checkpoints, generations, audit, deletion | One row per profiler event/value/log | Hosted transactional authority |
| SQS | Small work pointers and retry timing | Evidence or durable progress truth | Replaceable delivery |
| ClickHouse | Rebuildable non-value population/retained-instance metadata and rollups | Any customer value/log body, preview, decoded value, scalar/path row or value index | Hosted analytical projection |

Detailed documents:

- [Local artifacts](local-artifacts.md)
- [Local control database](control-sqlite.md)
- [S3](s3.md)
- [PostgreSQL](postgres.md)
- [ClickHouse](clickhouse.md)

## Schema honesty

The current branch has no Project Studio **control.sqlite**, immutable Studio spool/uploader, PostgreSQL migrations, or ClickHouse migrations. The source documents specify table/column inventories, but not complete executable DDL: many physical types, nullability rules, keys, indexes, codecs and view definitions remain unfrozen. The spool’s literal path and local control schema are also freeze gates.

These documents therefore do two things:

1. reproduce every known table/column and settled semantic rule; and
2. label missing physical detail as a v1 freeze gate instead of inventing it.

The public logical catalog is versioned and stable. Physical database names are private provider mappings.
