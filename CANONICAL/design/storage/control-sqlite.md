# Local control database

**Status:** Target v1 responsibility boundary. **control.sqlite** and the immutable Project Studio spool do not exist on this branch. The source material does not specify literal table/column DDL or a canonical filesystem path, so this document does not invent either.

## Why it is separate

The local host has two very different kinds of state:

- canonical evidence and value bytes under **.baml**; and
- transactional obligations such as “this immutable chunk is still owed to hosted storage.”

Rebuildable SQLite/Parquet/direct-artifact query providers can be deleted and regenerated. Upload obligations cannot. **control.sqlite** is the proposed small non-rebuildable authority for those obligations; it is never an analytical catalog or a copy of evidence bodies.

## Known responsibilities

The design requires durable local state for:

- immutable spool-object ownership, identity, path, length, checksum, and lifecycle;
- upload authorization identity/expiry and exact object-key/header binding;
- uploaded object key/version/checksum resolution after ambiguous responses;
- manifest commit and receipt-anchor identity/state;
- per-stream committed, anchored, and client-reclaimable contiguous watermarks;
- capture/index/retention/durability policy pointers used by the host;
- pending reconstruct/reindex/export/erasure or synchronization operations that are truly local obligations;
- local-to-hosted run/artifact attachment identity; and
- idempotency, retry scheduling, bounded errors, and reconciliation cursors.

These are responsibilities, not approved table names. The exact division into tables and columns is a v1 schema freeze gate.

## Forbidden contents

**control.sqlite** must not become:

- the sole copy of profiler artifacts or canonical CAS nodes;
- a row-per-event, row-per-value-node, log, or body store;
- the public SQL catalog/provider by accident;
- hosted authorization or commitment authority;
- a replacement for S3 receipts or PostgreSQL; or
- a place where a browser can write arbitrary paths/SQL.

## Transaction and crash invariants

1. A spool item is upload-ready only after its immutable bytes and ownership state survive a crash; the final file/row publication sequence must be specified and crash-tested.
2. Retry never changes immutable chunk identity, bytes, checksum, ledger date, or authorized object target.
3. Ambiguous object-store responses resolve by exact key/version/length/checksum before commit or retry.
4. Local reclamation advances only through `min(contiguous_committed_through, contiguous_anchored_through)`.
5. A later successful sequence cannot cover an earlier gap.
6. Startup reconciliation handles file-without-row, row-without-file, expired authorization, uploaded-uncommitted, committed-unanchored, and already-anchored-but-local cases explicitly.
7. Removing a provider cache cannot modify this database; removing evidence cannot strand an unrecorded hosted obligation.
8. Database corruption, migration failure, disk-full, and permission failures produce a typed capture/synchronization state rather than silent byte deletion.

## DDL and layout freeze checklist

Before implementation, publish:

- literal database/spool paths and project-versus-user scope;
- schema/version table and forward migration/backup rules;
- spool, authorization, object, commit/receipt, stream-watermark, policy, attachment, operation, and reconciliation tables—or an explicitly justified smaller split;
- every column type, nullability, default, key, unique constraint, state transition, and index;
- the file/database durability publication protocol and fsync boundaries;
- single/multi-process ownership, leases, fencing, and lock recovery;
- maximum database/spool growth and admission behavior;
- retention/erasure/CAS-pin interaction;
- credential and sensitive-error handling; and
- crash/fault-injection fixtures for every invariant above.

The chosen schema must remain small and operational. If a proposed table is rebuildable purely from canonical artifacts, it belongs in a provider cache instead.

Related: [Capture and ingest](../05-capture-and-ingest.md), [Local artifacts](local-artifacts.md), [S3](s3.md), and [PostgreSQL](postgres.md).
