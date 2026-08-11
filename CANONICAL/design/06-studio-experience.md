# Studio experience

**Status:** A current runs/run-detail observability UI and BQL-backed query path exist. The unified command surface, DataFusion SQL, full v1 explorer and hosted API are target work.

## Current branch

The present implementation has:

- **baml playground**;
- a separate **baml studio** command;
- **baml q** for BQL;
- private **/api/obs** WebSocket requests returning BQF1 frames;
- methods for runs, run metadata, timeline, left-heavy flame, top functions, recent calls and BQL;
- a runs list and run detail UI with timeline/flame/top-functions and captured-value flows; and
- a separate legacy value-read WebSocket path.

It does not have:

- **baml query**;
- a DataFusion query crate/provider;
- the complete API listed below;
- a unified command surface; or
- the hosted Project Studio system.

Current compatibility surfaces stay until their replacements reach parity.

## Target product surface

Project Studio is the initiative name. The target local commands are:

- **baml playground** — serve/open the local browser debugger and expose private UI RPC;
- **baml query** — versioned portable SQL for users, agents and scripts.

No separate Studio application is required after consolidation. The old command is removed only after routing/behavior parity.

## Core user workflows

V1 requires the workflows below, but not five equally polished dedicated screens. The browser P0 is a retained-operation explorer plus run debugger. Capture health may initially be badges and **doctor** output; comparison may be SQL plus debugger links; reconstruct/reindex/export/erasure may be CLI/API operations with progress. Dedicated comparison, capture-health, and operations screens are deferred product depth.

### Explorer

- recent terminal, durable running and stale-open retained operations;
- filters for status, function/definition, revision/release, environment, provider/model and bounded application tags;
- availability, loss and projection badges beside results;
- stable token cursors, never offsets;
- selecting an operation opens its containing run without losing list position.

Population questions use population relations. The explorer never presents retained-instance count as fleet prevalence.

### Run debugger

- call tree, logical threads and spawn graph;
- timeline and flame views over one semantic run;
- inclusive/self/await timing;
- LLM/provider/tool/agent/resource facts only when emitted;
- retained values/logs with lazy budgeted reads;
- source and revision identity;
- exact-window and capture-loss evidence;
- six independent state axes;
- copyable run/call/value/source identifiers.

Large trees use aggregation, collapse, incremental fetch and virtualization. A UI never makes one DOM node per event.

### Capture health

- profiling/capture mode;
- ring/value/log pressure and losses;
- local spool bytes/age;
- upload/receipt watermarks;
- compatibility/unsupported versions;
- projection delay/quarantine;
- actionable doctor/runbook links.

### Comparison

Minimum v1 supports bounded run/cohort comparison through SQL and selected debugger links. A fully polished comparison screen is later product depth.

### Operations

Core v1 exposes reconstruct, reindex, bounded export and explicit deletion progress. Durable background queries, rerun and test creation are deferred.

The erasure capability is required, but its exact command and HTTP route are a v1 interface freeze gate. The frozen request must carry an authorized supported scope (tenant/project/environment/run), idempotency identity, and legal-hold acknowledgement; return an operation identity; deny logical access first; expose per-store progress; and reach **verified_deleted** only after verification. Do not ship an ad-hoc local file deletion command as a substitute.

## Natural-language flow

~~~text
user question
-> agent reads capabilities + schema
-> agent writes portable SQL
-> engine binds fixed scope/snapshot
-> agent reads rows + mandatory outcome
-> agent opens selected run/value/source IDs
-> agent explains evidence and unknowns
~~~

The agent never parses decorative terminal tables or sees physical database names. It never runs an LLM inside the query engine.

## Target CLI

### Read/query

~~~text
baml playground capabilities --format json
baml playground inspect [PATH]
baml playground artifacts list|validate [--deep]
baml playground observations list|show
baml playground runs list|show|graph|profile
baml playground values read <ref>
baml playground logs list <run>
baml playground source show <snapshot> [--file ... --span ...]

baml query --schema [--view <name>] --format json
baml query "<sql>" --format json|jsonl
baml query "<sql>" --hosted
baml query "<sql>" --both
~~~

### Local service/maintenance

~~~text
baml playground [serve]
baml playground tail
baml playground doctor [--deep]
baml playground upload --to <profile>
baml playground reconstruct [--output ...]
baml playground reindex
baml playground diff <run-a> <run-b>
baml playground export --format json|jsonl|parquet|otlp
~~~

### Deferred

~~~text
baml playground rerun <run>
baml playground test create <run>
durable/background query operations
~~~

## Output contract

- Data rows/structured results to stdout.
- Diagnostics/progress and human query outcome to stderr.
- JSON = one versioned envelope.
- JSONL = schema-declared data frames plus a mandatory terminal control frame.
- Human cosmetics may change; JSON meaning requires versioning.
- IDs are copyable and resolvable by a follow-up command.
- Exit codes distinguish success, no match, corrupt/unsupported evidence, invalid SQL, authorization, transport, budget, cancellation and internal failure.

A partial/incomplete outcome cannot masquerade as successful complete output.

## Private RPC

The UI remains free to use fold-engine-shaped methods instead of routing through SQL. Target method families:

- run/observation list;
- run snapshot + patches;
- CCT graph/profile/timeline;
- value list/read;
- logs/events;
- source/schema.

This is private, versioned with the UI and allowed to change. It must agree with CLI/SQL semantics where they overlap.

## Public HTTP API

### Capabilities/query

~~~text
GET    /v1/capabilities
GET    /v1/query/schema?version=v1
POST   /v1/query/sql
DELETE /v1/query/sql/{query_id}
~~~

Request:

~~~json
{
  "version": "v1",
  "sql": "SELECT ...",
  "cell": null
}
~~~

The API authenticates, binds QueryScope/snapshot, invokes DataFusion/BAML, streams rows and emits the mandatory typed outcome. The browser/CLI never receives ClickHouse credentials.

### Private browser reads

~~~text
GET  /v1/observations/{id}
GET  /v1/observations/{id}/events
GET  /v1/runs/{id}
GET  /v1/runs/{id}/snapshot
GET  /v1/runs/{id}/patches?after=<cursor>
GET  /v1/runs/{id}/graph
GET  /v1/runs/{id}/profile
GET  /v1/runs/{id}/logs
GET  /v1/runs/{id}/values/{value_id}
POST /v1/runs:diff
GET  /v1/program-snapshots/{id}
GET  /v1/program-snapshots/{id}/files/{path}
GET  /v1/schemas/{schema_id}
~~~

### Ingest

~~~text
POST /v1/ingest/sessions
POST /v1/ingest/sessions/{id}/authorizations
POST /v1/ingest/sessions/{id}/chunks:commit
POST /v1/ingest/sessions/{id}:complete
GET  /v1/ingest/sessions/{id}/status
~~~

### Core operations

~~~text
POST /v1/runs/{id}:reconstruct
POST /v1/projects/{id}:reindex
POST /v1/exports
GET  /v1/operations/{id}
POST /v1/operations/{id}:cancel
~~~

The exact erasure-create route/command is intentionally not named here; it is an interface freeze gate with the semantic contract above. Rerun, create-test and durable query-job endpoints do not enter v1 until their deferred decisions close.

## Capabilities

Capabilities advertise:

- API version;
- SQL catalog/view versions;
- readable artifact/envelope versions;
- observation/event kinds/fields;
- available datasets;
- query and value-read capabilities;
- rerun/test/export availability;
- active projection generation;
- compatible cursor versions; and
- local capture-adapter capabilities.

Clients use capabilities rather than server-version guessing.

## Errors

~~~text
invalid_request
invalid_sql
unsupported_capability
E_BACKEND_CAPABILITY
authorization_denied
not_found
artifact_corrupt
artifact_unsupported
projection_delayed
E_QUERY_BUDGET_EXCEEDED
rate_limited
dependency_unavailable
conflict
cancelled
internal
~~~

**E_BACKEND_CAPABILITY** and **E_QUERY_BUDGET_EXCEEDED** are decision-frozen query codes. The remaining generic API code spellings freeze with the v1 error schema. Errors include a stable code, human message, request/query ID, retryability and bounded structured details. They never contain secrets, presigned URLs, raw customer bodies or private physical SQL.

## Live updates

Patch kinds:

- observation/call/thread upsert and terminalize;
- graph edge addition;
- value/log availability change;
- evidence-state change;
- diagnostic addition;
- run state change.

Each patch has a monotonic per-run semantic sequence and durable watermark. Pre-flush patches may be volatile.

Reconnect:

1. client supplies optional durable cursor;
2. server returns one snapshot at a known cursor;
3. server sends newer patches only;
4. expired/compacted/future cursor gets typed resnapshot guidance;
5. slow client is disconnected with latest recoverable cursor rather than buffered indefinitely;
6. client rejects duplicates, backwards sequence and gaps.

Hosted notifications are wake hints. Durable state/cursors remain authority; no Redis/event bus is required for correctness.

Ordinary SQL is fixed-snapshot and does not tail this stream.

## Local browser security

- loopback or Unix socket only;
- Host and Origin validation;
- no wildcard CORS;
- one-time browser handoff exchanged for rotated HttpOnly SameSite cookie;
- explicit consent before a hosted origin connects to a local agent;
- no browser-supplied arbitrary filesystem paths;
- budgeted value reads;
- audit when policy/hosted profile requires it.

Server-backed browser sessions additionally need per-session uid/container isolation, read-only root filesystem, no table functions/network/filesystem escape, engine/library integrity checks, CPU/memory/disk quota and TTL cleanup.

## UI performance rules

- virtualize lists/trees/logs/value tables;
- Canvas/WebGL or aggregate tiles for dense timelines/flames;
- summarize beyond an explicit node threshold;
- lazy/range-read bodies/source;
- cancel obsolete requests;
- byte-bound caches;
- paginate/stream large results;
- bounded hover prefetch; and
- preserve layout while showing progressive/incomplete state.

Exact thresholds and SLO values are benchmark-owned.

## Migration from current UI

1. Keep current **/api/obs** and BQL while building catalog/providers.
2. Add new schema/query endpoints and terminal outcome.
3. Make the current run viewer consume canonical private RPC DTOs.
4. Add availability/state badges and durable-running semantics.
5. Reach CLI/RPC/SQL conformance.
6. Move users to **baml query**.
7. Delete BQL/BqlTable only after parity gates.
8. Route/remove separate **baml studio** only after **baml playground** parity.
