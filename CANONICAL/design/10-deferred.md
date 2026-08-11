# Deferred

**Status:** Explicitly outside v1, policy-unfrozen, or awaiting a separate decision. Superseded designs are not listed as deferred alternatives.

## Query and value policy

### X1 — Exact policy values

Defer the numeric/product choices for:

- exact versus summary/omitted capture defaults by environment and value role;
- path allowlists, denylists, redaction and sensitive-field handling;
- maximum hydration depth, nodes, array elements, string bytes and decoded bytes;
- object requests/bytes per query;
- duration, memory/spill, output, concurrency and tenant quotas;
- optional future customer-configured hosted retention windows and policy migration; and
- whether any external value-search index outside ClickHouse should exist.

The v1 schemas must still carry policy/version and explicit **not_captured**, **redacted**, **truncated**, **query_budget_exhausted**, and future **not_indexed** states.

### X2 — Default Studio search grain

Occurrence, distinct-root and scalar/path grains are all supported. Which one Studio shows first is a later UX decision.

### X3 — Durable background query operations

Deferred:

- surviving client disconnect or worker restart;
- durable progress/checkpoints;
- low-priority job queues;
- persisted large result sets; and
- resumable job UX.

This is not required merely because a query scans many values. Ordinary queries stream until complete, cancelled, or budget-exhausted.

### X4 — Hosted opaque value reference

Still unresolved:

- raw canonical CID;
- deterministic tenant-scoped token;
- random occurrence reference;
- public syntax/stability;
- equality-query availability;
- key rotation; and
- local/hosted comparability.

No v1 schema or example may assume one before this decision closes.

### Strict unavailable-value mode

The default preserves typed unknown rows/evaluations and returns an incomplete outcome. A future “fail on any unavailable value” mode is deferred.

### User query functions

User-defined UDFs, CREATE FUNCTION, BAML functions registered in SQL, plugins and arbitrary code are prohibited in v1. A later extension needs a new sandbox/security/cost decision.

### External value index

ClickHouse cannot be the value index. A future text/vector/scalar index is a separate service with explicit authorization, retention, deletion, consistency, cost and evidence semantics.

## Product depth

- Durable rerun with reproducibility and side-effect prerequisites.
- Reviewable test generation.
- Full five-screen product beyond the initial run viewer.
- Final search-result presentation.
- Collaboration, annotations, replies and sharing workflows.
- Scoring, prompt management and general evaluation-suite features.
- Billing and entitlement product behavior.
- In-BAML observability/query API.
- MCP or a public query-language service beyond CLI/HTTP SQL.
- Tenant-dedicated raw physical database access.

## Language/runtime-dependent depth

- Provider-attempt, tool, agent and resource observations until language-owned versioned records land.
- Effective-schema overlays and complete historical type-aware queries.
- First-class application user/session dimensions beyond bounded tags.
- Production wiring for speculative helper-value promotion.
- A bounded full-trace writer and its exact budget contract.
- Final cross-host capture-mode/configuration surface.

## Hosted and enterprise

- Multi-cell stream cutover and cross-cell ad-hoc SQL.
- Cross-region anchored durability tier and final RPO/RTO.
- BYOK/application envelope encryption.
- Kubernetes/Helm packaging.
- Broad multi-cloud control plane.
- Contract-specific “one command enterprise” packaging.

## Benchmark-owned, not deferred semantics

These choices must be measured during v1 implementation:

- SQLite versus Parquet versus direct-artifact provider per logical relation;
- ClickHouse physical table split, ORDER BY, partitioning, projections and codecs;
- active-index engine and TTL;
- chunk size/age/record thresholds;
- projector batch and throughput targets;
- cell admission/recovery headroom;
- query memory/spill/concurrency defaults;
- final SLO/error-budget values; and
- UI virtualization/cache thresholds.

The semantic contract is already fixed. Benchmarks choose an implementation without changing user-visible meaning.

## Current implementation gaps that are not product deferrals

These are required work, not optional later features:

- implement **baml_query** and the public catalog;
- freeze column types/nullability and row-level unavailable semantics;
- implement local providers and the canonical ValueResolver;
- implement mandatory query outcomes;
- build hosted ClickHouse/S3 providers;
- build PostgreSQL/ClickHouse migrations;
- build receipt-backed hosted ingest and reconciliation;
- implement the selected structural-exhaustion policy;
- preserve/fix current profiler durability and memory invariants;
- preserve exact population semantics across folded counter/histogram overflow;
- replace BQL only after parity; and
- consolidate commands only after replacement behavior ships.

## Superseded, not deferred

Do not revive these as “maybe later” without a new decision:

- BQL as public product language;
- StudioQueryV1;
- direct public ClickHouse dialect;
- chDB as the architectural local engine;
- per-call population rows;
- customer value content/previews/path rows in ClickHouse;
- automatic hosted expiry as the default;
- a fixed tenant CID-token representation;
- the PR prototype’s SHA-256 JSON value store; or
- DataFusion-free hosted public SQL.
