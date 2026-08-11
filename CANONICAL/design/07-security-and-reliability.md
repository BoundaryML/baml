# Security and reliability

**Status:** Canonical v1 requirements. The local profiler substrate is built; the hosted controls, migrations, failure-injection suite, and operational runbooks are target work. Numeric limits and SLOs below are qualification targets, not measured claims.

## Non-negotiable properties

Project Studio must preserve five properties across local and hosted operation:

1. **Tenant isolation.** A request cannot name, infer, hydrate, export, or erase evidence outside its authorized tenant/project/environment scope.
2. **Evidence honesty.** Missing, redacted, lost, corrupt, delayed, or budget-exhausted evidence is represented explicitly; it never becomes an ordinary NULL, false predicate, or successful complete answer.
3. **No acknowledged silent loss.** Bytes become reclaimable only after an immutable object, a committed manifest, and a verified receipt form one contiguous durability watermark.
4. **Bounded work.** Capture, decode, projection, query, hydration, export, and deletion all have explicit limits, cancellation, and backpressure.
5. **Rebuildability.** Losing a projection, queue, or local analytical cache does not lose accepted evidence or transactional truth.

## Data classification and placement

Treat all customer-derived metadata as tenant data, including function names, source paths, errors, timings, model/provider identifiers, query text, and opaque value handles. Value and log bodies are the most sensitive class.

| Data | Allowed authority | Forbidden placement |
|---|---|---|
| Local exact evidence and values | Sealed **.baml** artifacts and local canonical CAS | Rebuildable provider state as sole copy |
| Hosted exact evidence and values | Accepted S3 artifact/CAS chunks | ClickHouse, PostgreSQL event rows, SQS bodies, logs |
| Transaction and authorization state | PostgreSQL; local **control.sqlite** for host-local obligations | ClickHouse as authority |
| Non-value analytical facts | ClickHouse, rebuilt from accepted evidence | Value bodies, previews, decoded leaves, path indexes |
| Work notification | SQS pointer containing only the minimum routing/work identity | Artifact payloads, secrets, value bodies |
| Presigned URL and credentials | In-memory, short-lived transport context | Application logs, UI diagnostics, analytical stores |

S3 object keys contain scoped identifiers and are themselves sensitive. Canonical content IDs can reveal equality; hosted value-handle representation and equality are therefore intentionally unresolved in v1 design work. See [Deferred X4](10-deferred.md#x4--hosted-opaque-value-reference).

## Authorization chain

~~~text
authenticated principal
-> control-plane membership/service-principal check
-> project routing epoch and cell/lane resolution
-> immutable QueryScope or operation scope
-> PostgreSQL forced RLS and scoped keys
-> trusted logical provider mapping
-> ClickHouse scoped identity + row policy + column grants
-> authorized private S3/CAS resolution
-> response filter + audit event
~~~

### QueryScope

Every query binds, before planning:

- principal and authorization decision identity;
- tenant, project, and allowed environment scope;
- cell/routing generation;
- logical catalog version;
- physical projection generation and projected-through barrier;
- source/provider snapshot handles;
- policy and budget profile; and
- cancellation/deadline context.

SQL cannot widen this scope. Physical table names, provider handles, object keys, and tenant predicates are injected only by trusted code. A query snapshot does not preserve access after explicit revocation or erasure: the coordinator cancels affected work.

### Defense in depth

- PostgreSQL tenant-owned keys carry tenant/project scope, and tenant roles use forced RLS.
- ClickHouse roles receive a scoped identity, row policies, serving views, and column grants; users never connect directly.
- S3 ingest credentials can create only the exact authorized key and cannot overwrite, list broadly, or delete.
- S3 read/delete access is service-side and scoped through PostgreSQL authority; opaque handles are not bearer capabilities.
- SQS consumers re-read authoritative PostgreSQL state and never trust a queue payload as authorization or progress.
- Browser clients receive no database credentials or long-lived object-storage authority.

The migration roles that alter PostgreSQL or ClickHouse schemas are separate from serving and projector roles.

## Query attack surface

The public surface is the platform-owned BAML/DataFusion SQL grammar and allowlisted function catalog.

V1 excludes:

- user-defined functions, **CREATE FUNCTION**, extensions, plugins, or arbitrary code;
- raw ClickHouse parser or passthrough SQL;
- caller-selected physical relations or provider mappings;
- ClickHouse file, URL, S3, remote, system-log, or process-table access;
- tenant-role temporary tables; and
- an LLM inside the execution plan.

All queries use:

- parser/planner complexity limits;
- statement and result limits;
- one query-global memory/time/rows/bytes/hydration budget;
- concurrency and workload admission;
- bounded async hydration with distinct-CID reuse;
- spill only to a scoped, encrypted or otherwise policy-compliant temporary area;
- backpressure from client through providers; and
- cancellation propagated to ClickHouse, S3 reads, decode, and output.

Candidate count alone never changes a synchronous query into a deferred job. The stream completes, is cancelled, or terminates with a typed budget/error outcome. Large durable background jobs are deferred.

## Integrity and evidence states

Integrity is end-to-end, not an S3 checksum alone:

1. the runtime emits checksummed, versioned, committed-prefix-readable artifacts;
2. a source-range chunk binds source identity, range, sequence, predecessor, envelope digest, and payload digest;
3. authorization binds exact object key, length, checksum, expiry, and required headers;
4. PostgreSQL commits immutable manifest fields idempotently;
5. a deterministic service-authenticated receipt anchors the accepted manifest set in S3;
6. projectors revalidate scope and content before deterministic projection; and
7. analytical rows carry provenance, deterministic identity/hash, and projection generation.

Conflicting immutable facts are quarantined. “Latest arrival wins” is not an integrity policy.

The product preserves independent execution, structural-completeness, value-availability, integrity, projection, and retention states described in [Product and lifecycle](01-product-and-lifecycle.md#evidence-and-availability). A terminal SQL stream also carries [the mandatory query outcome](04-query-system.md#terminal-outcome). Missing that outcome means the caller did not receive a successful complete result.

## Durability protocol

### Local

- Active BCCT/BMET readers observe only checksummed committed prefixes.
- Sealed artifacts are immutable.
- CAS pack bytes must be durable before a boundary root is reclaimable.
- Boundary and upload pins participate in local GC reachability.
- Local retention appends tombstones and never treats a rebuildable index as evidence.

The current CLI fsyncs a CAS pack before appending **manifest.bamlcids**, but does not fsync that manifest append in the same barrier. Closing this root-pin durability gap is a v1 correctness gate.

### Hosted

The client may reclaim through:

~~~text
min(contiguous_committed_through, contiguous_anchored_through)
~~~

A later sequence cannot cover an earlier hole. Uploaded-but-uncommitted objects are not accepted evidence. Committed-but-unanchored manifests are repaired before acknowledgment. Accepted S3 evidence is immutable and retained indefinitely by default; normal age/size maintenance cannot evict it.

## Failure matrix

| Failure | User-visible truth | Required recovery |
|---|---|---|
| Producer/ring structural loss | Run/partition becomes degraded with loss markers; population attribution may be incomplete | Resynchronize at a valid record boundary; never wedge or silently drop time |
| Process dies mid-boundary | Durable begin with no terminal becomes partial/abandoned after liveness evidence | Read committed prefixes; preserve captured evidence |
| Local disk full or permission failure | Capture/durability state is explicit; the run follows the selected exhaustion policy | Stop growth, preserve prior durable bytes, surface diagnostics |
| CAS pack durable, root pin torn | Current implementation gap; value may be vulnerable to GC | Atomic/durable pin protocol plus crash-injection test |
| Upload times out ambiguously | Do not upload a different object for the same identity | Resolve by exact key/version/checksum before retry/commit |
| S3 object exists, PG commit absent | Unaccepted orphan; invisible to queries | Grace-period inventory reconciliation, then commit if authorized or delete safely |
| PG commit succeeds, receipt write fails | Not client-reclaimable; committed state remains authoritative | Receipt repair and anchored watermark advance |
| Outbox publish/SQS delivery lost or duplicated | No evidence loss; projection may be delayed | Republish from outbox; projector re-reads PG and is idempotent |
| Projector crashes or lease expires | Checkpoint does not advance past unverified output | Fenced lease, deterministic replay, read-back on ambiguous insert |
| ClickHouse unavailable or lost | Hosted SQL may be unavailable/degraded; accepted evidence remains | Rebuild active generation from S3 under PG commitments |
| S3 value range unavailable/corrupt | Candidate row retains typed unavailable/unknown state; outcome is incomplete | Retry within budget, quarantine/repair, report reason |
| Client disconnects | No orphan query continues indefinitely | Propagate cancellation; release memory, spill, provider and hydration work |
| Authorization revoked during query | Snapshot cannot preserve access | Cancel affected query and deny subsequent object reads |
| Explicit erasure races with query/projection | Logical access denied first | Cancel, tombstone, purge all stores/caches/exports, verify |
| PostgreSQL restored behind S3 receipts | Some accepted commits are missing from restored ledger | Import and verify authenticated receipts, then reconcile projections |

## Explicit erasure

Erasure is a state machine, not a best-effort DELETE:

~~~text
requested -> authorized -> access_denied -> deleting
-> backup_expiry_pending (when applicable) -> verified_deleted
                         \-> failed/retryable
~~~

The operation:

1. freezes the exact tenant/project/environment/run scope and legal-hold decision;
2. denies logical reads and new ingestion in scope;
3. cancels active queries, exports, projection, and upload work;
4. tombstones PostgreSQL authority and schedules idempotent store tasks;
5. deletes accepted S3 artifacts/CAS, derived snapshots, caches, exports, and provider state;
6. deletes or rebuilds affected ClickHouse projections;
7. addresses replicas and backups under the declared policy; and
8. verifies every required store before recording completion.

Ordinary retention is not erasure. Hosted customer-defined expiry windows, legal-hold policy, and backup-expiry numbers remain policy freeze work.

## Audit contract

Audit records are append-only, scoped, integrity protected, and kept outside analytical customer evidence. They cover at least:

- sign-in/service-principal and authorization decisions;
- membership, credential, routing, policy, and retention changes;
- upload authorization, manifest commit, conflict/quarantine, and receipt anchor;
- every public SQL request, including catalog/snapshot, normalized query identity, outcome, and resource summary;
- value, source, artifact, export, and private run-detail reads;
- projection generation activation, replay, and repair;
- deletion/legal-hold lifecycle; and
- privileged operational access.

Raw query text may contain customer literals and is classified tenant data. The exact redaction/encryption/retention policy for query text is a policy freeze gate; do not send it to ordinary service logs.

## Observability of the observability system

Keep three planes distinct:

| Plane | Examples |
|---|---|
| Service health | request latency/errors, queue age, worker lease churn, ClickHouse/S3/PG dependency health |
| Evidence health | ring loss, degraded boundaries, commit/receipt gaps, projection lag, quarantine, unavailable-value reasons |
| Resource/admission | spool bytes, CAS/live-ring memory, query concurrency/memory/spill, hydration bytes/requests, projector headroom |

Alerts should be based on user-impacting invariants: oldest unanchored contiguous gap, oldest unprojected accepted commit, silent-loss count, quarantine growth, erasure age, and tenant-isolation test failure. Queue depth alone is diagnostic, not authority.

## Qualification targets

These targets came from the design corpus and require corpus-scale measurement before release:

| Path | Qualification target |
|---|---:|
| Local live interaction | p95 under 250 ms |
| Local sealed-file query | p95 under 750 ms |
| Hosted commit-to-queryable | p50 under 2 s, p95 under 5 s, p99 under 15 s |
| Hosted run detail | p95 under 1 s |
| Hosted fleet query | p95 under 3 s |
| Acknowledged silent evidence loss | zero |

The exact workload, result size, cache state, tenant shape, and concurrency for each target must be published with the benchmark. Until then these are gates, not product SLO claims.

## V1 validation and release gates

- Codec/container golden tests, fuzzing, torn-write recovery, unknown-version/kind behavior, and hostile decode limits.
- Profiler stress tests for recursion, thread churn, loss resynchronization, memory caps, crash recovery, retention, CAS root durability, and GC races.
- Local/hosted conformance corpus with pushdown on/off, unavailable values, running rows, cancellation, limits, aggregates, and query-outcome parity.
- Cross-tenant tests at API, PostgreSQL RLS, ClickHouse row-policy/grant, opaque-handle, S3 range-read, cache, export, audit, and deletion boundaries.
- Upload/receipt/outbox/projector failure injection for every row in the failure matrix.
- Real-provider S3 compatibility tests, not only mocks.
- Projection rebuild and generation rollback from an empty ClickHouse deployment.
- PostgreSQL backup/restore followed by receipt import and full reconciliation.
- Erasure verification across live stores, replicas, caches, exports, and declared backup lifecycle.
- Capacity/admission tests that reserve ingest/projector recovery headroom while queries are saturated.
- Canary and rollback runbooks with named owners, dashboards, alerts, and tested stop conditions.

No component is “production ready” because its happy path works. V1 release requires the end-to-end evidence, isolation, and recovery invariants above.
