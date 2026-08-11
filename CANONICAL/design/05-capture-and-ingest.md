# Capture and ingest

**Status:** Local native capture/artifact writing is built. The Project Studio immutable spool/**control.sqlite**, uploader, envelope, receipt-backed commitment, projector, and reconciliation system are target v1 work and do not exist on this branch. The literal spool path and **control.sqlite** schema are not yet frozen.

## Boundary: capture is not upload

~~~text
BAML instrumentation
-> host-specific drain adapter
-> optional durable spool
-> optional upload transport
~~~

These responsibilities may share a process, but their interfaces and failure states stay separate.

### Instrumentation

Owns BAML identity/semantics and emits structural facts plus separately managed value/log captures. It never sees cloud credentials, hosted retry policy, tenancy or analytical schemas.

### Drain adapter

Drains complete records, preserves ordering/provenance, batches them into local artifacts or record-aligned chunks, and reports pressure.

Possible placement:

- native background thread;
- standalone process tailing sealed artifacts;
- sidecar/extension;
- serverless in-process adapter;
- cooperative wasm callback.

### Durable spool

Where the host has durable storage, the spool owns:

- same-filesystem temporary write;
- fsync;
- atomic rename;
- parent-directory fsync;
- transactional ownership in control.sqlite;
- immutable retry bytes; and
- reclaim only after a receipt-backed contiguous watermark.

This is a target component, not the existing **.baml** history writer. Its filesystem layout and control-database migrations freeze with the hosted evidence milestone; no path is invented in this document.

### Upload transport

Obtains bounded authorization, uploads immutable chunks, commits manifests and retains local bytes until durable acceptance. It is optional for local-only use.

## Capture modes

| Mode | Promise | Failure behavior |
|---|---|---|
| **off** | No evidence | Nothing to preserve |
| **diagnostic** | Bounded best-effort evidence; incompleteness explicit | Application may continue |
| **delivery_required** | Operation is not successfully observed until evidence is durably accepted | Wait within budget, then fail/mark operation per policy |
| **durable_spool** | Admitted structure survives network failure after local durable write | Stop admission and apply structural-exhaustion policy when spool fills |

A host with neither memory nor durable storage cannot promise lossless asynchronous capture. It must choose off, diagnostic, or wait for remote durable acceptance.

Exact environment defaults are policy-deferred.

## Host matrix

### Native process, VM or container

~~~text
runtime rings/value queues
-> native drain
-> .baml artifacts + immutable spool
-> embedded/standalone uploader
~~~

- No network on the hot path.
- Structural and value/log budgets remain independent.
- Network outages grow bounded spool.
- Ephemeral container disks are durable only to the platform guarantee actually configured.

### AWS Lambda

~~~text
in-process drain
-> memory chunk builder
-> optional /tmp staging
-> batched upload/commit before handler success when delivery_required
~~~

**/tmp** is not the durability boundary. A Lambda extension may improve batching but cannot guarantee work after handler return/forced shutdown. No per-event requests.

### Edge isolate

~~~text
cooperative wasm drain
-> bounded in-isolate chunk
-> application-owned fetch/queue/object adapter
~~~

No sidecar or durable local filesystem. Strong delivery waits for a durable remote acknowledgement. Diagnostic mode may lose the abrupt tail and must record that guarantee up front.

### Browser/embedded wasm

Diagnostic cooperative capture only is the v1 posture. The embedding application supplies transport if it wants durability. The old OPFS/IndexedDB durable-spool design is not v1.

### Local CLI/tests/offline

No upload required. Readers do not mutate evidence to make it queryable.

## Performance rules

1. No per-event HTTPS, S3, SQS, PostgreSQL, ClickHouse or fsync.
2. Structural and value/log queues stay separate.
3. Reserve before copy/encode.
4. Large bodies become references, not structural payload.
5. Drain complete records in batches.
6. Close chunks by both age and size.
7. Bound upload concurrency by bytes and outstanding authorization.
8. Send semantic UI patches, not whole-run rebuilds per event.
9. Measure application CPU, allocation, latency, memory and failure impact.

Candidate chunk tuning remains benchmark-owned: 8–32 MiB, 250–1000 ms, 50k–250k records, capped by the 64 MiB protocol maximum.

## Current local reality

- Native profiling is default-on.
- Current ring exhaustion aborts; graceful structural policies are not wired.
- CLI history is best-effort and does not fail the application on observability filesystem errors.
- Session/history/CAS artifacts are produced locally.
- Full-trace capture is not implemented.
- The value drain service exists, but the CLI drains synchronously at boundary finish.
- Speculative staging APIs exist, but production helper-draft staging is not wired.
- No Project Studio artifact/CAS uploader or hosted envelope is implemented.

Before claiming **delivery_required** or **durable_spool**, v1 must implement the spool/control database, uploader/commit/receipt watermark, structural-exhaustion policy, and crash-safe durability barriers. Full trace and helper-draft promotion remain separate deferred product depth; synchronous draining may be replaced for latency/host guarantees but is not, by itself, proof that a spool is non-durable.

## Structural exhaustion target

### Values/log queue full

Skip the lower-priority body/record, increment a non-overlapping loss counter, preserve structural facts and show a precise availability state. Never render the absent value as NULL.

### Structural drain behind

Grow only within the admitted budget, wake bounded drain work and report pressure. No network work enters the producer path.

### Spool full and network unavailable

1. Stop admitting new captured runs.
2. Reserve capacity to close admitted runs.
3. Surface pressure through health/doctor/UI.
4. Attempt bounded upload/reclamation.
5. At the hard boundary, apply the preselected policy:

| Policy | Application | Evidence |
|---|---|---|
| **fail_run** recommended | Typed observability-capacity failure; host stays alive | Retain evidence through failure; terminal incomplete/failed |
| **abort_process** opt-in | Terminate after fatal diagnostic | Strict never-continue-unobserved stance |
| **continue_incomplete** diagnostic only | Application continues | Permanently mark structural incompleteness |

The runtime never silently weakens a complete guarantee to diagnostic.

### No storage and no remote

- delivery_required: fail the observed operation if acceptance cannot complete inside the invocation budget;
- diagnostic: retain a bounded window, report the undelivered range, permit application success.

### Producer killed

On the next read/sync, a durable open marker without completion becomes abandoned/incomplete. A torn last record is ignored and the intact prefix remains. Nothing invents success or failure.

## Chunk envelope

See [S3](storage/s3.md#artifactchunkenvelopev1) for literal fields and hard limits.

Required properties:

- one source artifact/range;
- deterministic versioned encoding;
- authenticated manifest binding;
- sequence/predecessor continuity;
- explicit capture policy and loss deltas;
- bounded compression/decode; and
- no scope trust from envelope content alone.

## Client upload lifecycle

1. Drain complete records into a source-range chunk.
2. Build the deterministic envelope.
3. Compress using the allowlisted protocol settings. V1 uses transport protection and provider-side object encryption; application-envelope encryption/BYOK is deferred.
4. Write/fsync immutable spool bytes where storage exists.
5. Obtain exact upload authorization.
6. Single-PUT with checksum and create-only semantics.
7. Resolve ambiguous PUT by object attributes/checksum or byte-identical retry.
8. Batch-commit uploaded manifests to the API.
9. Retain/reclaim local bytes only through the receipt-backed contiguous watermark.

## Commit and receipt

The API’s short transaction validates authenticated scope, authorization, object key/version, length/checksum, quota, immutable identity and manifest syntax without decoding the object.

It then:

- inserts/idempotently resolves the chunk;
- rejects conflicts;
- creates projection requirements;
- creates a pending deterministic receipt;
- advances contiguous commitment only;
- records audit/accounting; and
- writes outbox work.

After commit, the service writes/verifies the deterministic receipt object and marks it anchored. Only then may the client see a durability acknowledgement.

## Outbox and queues

SQS messages are small untrusted pointers:

~~~json
{
  "version": 1,
  "tenantId": "...",
  "projectId": "...",
  "environmentId": "...",
  "cellId": "...",
  "laneId": "...",
  "ledgerDate": "2026-08-10",
  "chunkId": "...",
  "projectionKind": "online",
  "projectionGeneration": 7,
  "enqueuedAt": "..."
}
~~~

Workers reload every scoped field from PostgreSQL. Duplicate, delayed, reordered or lost messages cannot change correctness.

Queue classes:

- online projection;
- replay/reindex;
- future durable/background value work;
- admin/export/deletion.

The third class has no required v1 durable-query product, but keeping workload isolation in the transport design avoids a schema rewrite later.

## Projector lifecycle

1. Receive pointer.
2. Reload authoritative committed requirement.
3. Acquire renewable lease with monotonic fence epoch.
4. Start at durable next_sequence and select a contiguous range.
5. Stream objects with bounded parallelism.
6. Verify object checksum/version, envelope digest, authentication tag, scope equality, decompression bounds, plaintext digest, record framing, source range and clock bounds. Any hard-limit/integrity violation quarantines the whole chunk; never partially accept it.
7. Restore a bounded state snapshot when needed.
8. Decode semantic population and retained-instance facts.
9. Clamp event times outside the policy window around **ledger_date** for partitioning/retention purposes and surface **clock_skew_flagged**; never let producer time choose arbitrary partitions.
10. Emit deterministic non-value ClickHouse batches whose boundaries are a pure function of committed ledger ranges.
11. Verify ambiguous writes by batch identity and row hashes using a replica-consistent readback.
12. Advance fenced checkpoint only after required visibility.
13. Delete SQS pointers already covered by disposition.
14. Emit best-effort wake hints.

No PostgreSQL transaction spans object or ClickHouse I/O. Stale workers never checkpoint after fence loss.

Terminal dispositions:

~~~text
projected
quarantined_corrupt
blocked_unsupported_version
suppressed_tombstoned
retryable_after(timestamp, reason)
~~~

## Reconciliation

Continuous reconciliation repairs:

- uploaded but uncommitted objects after a delete-intent grace protocol;
- committed chunks missing outbox/publication;
- published work missing terminal checkpoint;
- expired leases;
- SQS/DLQ expiry;
- stream gaps/completion disagreement;
- ambiguous/conflicting ClickHouse batches;
- tombstoned scopes still receiving work;
- obsolete projection generations;
- receipt gaps;
- commitment/checkpoint compaction lag; and
- quarantine retention.

Deleting an orphan is race-sensitive: create a PostgreSQL delete intent, reject a concurrent commit for that authorization, recheck the ledger, then apply a versioned delete marker. Alert if committed evidence ever receives that marker.

## Backpressure

Track pressure in order:

1. local uncommitted spool bytes/oldest age;
2. committed-but-unprojected bytes/age;
3. projector decoded-byte backlog;
4. ClickHouse insert/merge/query pressure;
5. PostgreSQL ledger/WAL/compaction pressure.

When capacity binds, preserve accepted chunks, cap projection before harming ClickHouse, pause authorizations, return 429/503 with Retry-After, and let producers retain spool. Query and projection workload classes remain isolated.

## Completion

Source completion names final length/digest/record count/sequence/loss totals. Run completion names every expected stream as required/optional/omitted/lost/unavailable and binds the explicit root attachment:

~~~text
boundary_id -> (process_id, engine_id, thread_id, call_id)
~~~

Completion is never inferred from idle time. Hosted **stale_open** is an observability classification only.

Older artifacts lacking manifests remain readable with downgraded, explicit states.

## V1 acceptance

- Injected process/queue failure cannot lose an acknowledged chunk.
- Spool never reclaims beyond the anchored contiguous watermark.
- SQS can be deleted and repopulated from PostgreSQL.
- ClickHouse can be deleted and rebuilt from S3/PG authority.
- Duplicate processing produces one semantic fact or an explicit conflict.
- Cross-scope envelope poisoning quarantines and audits.
- Unsupported/corrupt objects remain exact and reprocessable.
- Every pressure boundary exposes counters/health and follows the declared mode.
- Reconciliation has metrics, SLO, alert and runbook.
