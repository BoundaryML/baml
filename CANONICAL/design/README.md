# Design map

This folder is organized by the question a reader is trying to answer. It is not ordered by the history in which the design was discovered.

## Product and system

1. [Product and lifecycle](01-product-and-lifecycle.md) — jobs to be done, user journey, evidence states, v1 boundaries.
2. [System architecture](02-system-architecture.md) — components, authorities, local and hosted data flow.
3. [Profiler](03-profiler.md) — capture records, CCT aggregation, identity, values, formats, performance, code map.
4. [Query system](04-query-system.md) — DataFusion/BAML SQL, public catalog, snapshots, pushdown, hydration, budgets, outcomes, examples.
   For the proposed schemas and concrete review queries, see [Data model and query examples](../PROJECT_STUDIO_QUERY_EXAMPLES.md).
5. [Capture and ingest](05-capture-and-ingest.md) — host modes, spool, chunks, receipts, outbox, projectors, reconciliation.
6. [Studio experience](06-studio-experience.md) — commands, UI, private RPC, HTTP API, live updates.
7. [Security and reliability](07-security-and-reliability.md) — tenancy, authorization, audit, deletion, failure semantics, validation.

## Storage

- [Storage index](storage/README.md) — authority and placement matrix.
- [Local artifacts](storage/local-artifacts.md) — verified **.baml** tree and binary formats.
- [Local control database](storage/control-sqlite.md) — target non-rebuildable spool/receipt/policy authority and explicit DDL freeze gaps.
- [S3](storage/s3.md) — canonical hosted objects, keys, receipts, snapshots, deletion boundary.
- [PostgreSQL](storage/postgres.md) — transactional schemas and known columns.
- [ClickHouse](storage/clickhouse.md) — non-value analytical datasets, columns, serving and security contract.

## Governance and execution

- [Decision register](08-decisions.md) — settled choices and superseded alternatives.
- [Delivery plan](09-delivery-plan.md) — what exists, what is next, implementation gates.
- [Deferred](10-deferred.md) — explicitly out of v1 or policy values intentionally not frozen.
- [Source map](11-source-map.md) — disposition of every document in the [source archive](../archive/README.md).
- [Implementation notes](12-implementation-notes.md) — decision ledger for freeze-gate resolutions and implementation-only choices made while executing the delivery plan.
- [Glossary](glossary.md) — shared vocabulary.

## Status vocabulary

| Label | Meaning |
|---|---|
| **Built** | Present in the current branch and verified against code. |
| **V1 contract** | Required semantics for v1; implementation may not exist yet. |
| **Benchmark-owned** | Semantics are fixed; a measured implementation choice remains. |
| **Freeze gate** | Required for v1 implementation, but exact names/types/limits were not fixed by the source material and are not fabricated here. |
| **Deferred** | Deliberately outside v1 or awaiting a separate product/policy decision. |

“Canonical” means the decision is current, not that the implementation is complete.
