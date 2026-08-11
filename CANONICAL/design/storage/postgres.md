# PostgreSQL

**Status:** Proposed logical inventory for v1. There are no Project Studio PostgreSQL migrations on this branch. Columns below are the complete known inventory from the design; physical types, nullability, constraints and indexes still require migration DDL.

## Responsibility

PostgreSQL owns transactional mutation, authorization, idempotency and workflow coordination:

- tenants/projects/environments and principals;
- routing regions/cells/lanes;
- program-snapshot ownership;
- ingest sessions, reservations, commitment and receipts;
- run attachment and current evidence state;
- outbox, projection leases/checkpoints/batches/generations;
- policy pointers;
- audit, deletion, legal hold; and
- later saved SQL/operations metadata.

It stores no row per profiler event, value node, text delta or log line.

## Logical databases

~~~text
studio_control
  identity, project/routing, global policy pointers, deployment registry

studio_cell_<cell_id>
  ingest/commit ledgers, runs, outbox/checkpoints/generations,
  cell-local policy, audit, deletion, operations
~~~

The first deployment may colocate both logical databases on one RDS Multi-AZ allocation. Before claiming a second cell as an independent failure boundary, give it a separate writer. No cross-database foreign keys or transactions.

## Type rules already settled

| Semantic type | Rule |
|---|---|
| Service identifier | UUIDv7 |
| BAML identity | Preserve its complete native fields; do not flatten into one string |
| Digest | Binary |
| Wall time | timestamptz |
| Artifact-relative time/sequence | Integer plus explicit clock metadata |
| Evolving state | Constrained text/lookup table, not PostgreSQL enum |
| Mutable workflow row | created/updated timestamps and monotonic version where relevant |
| Tenant-owned key | Include tenant and project scope in PK/unique/FK |

Exact PostgreSQL type names and nullability per column are a freeze gate.

## Identity and routing

### tenants

Columns were not specified. Minimum responsibility: tenant identity and state. Do not fabricate the schema.

### projects

~~~text
tenant_id
project_id
home_region
state
routing_epoch
policy_id
~~~

### environments

~~~text
tenant_id
project_id
environment_id
name
retention_policy_id
~~~

### project_lanes

~~~text
tenant_id
project_id
routing_epoch
lane_id
cell_id
state
~~~

### memberships, service_principals, credentials

Names/responsibilities are specified; literal columns are not. Schema freeze required.

## Program snapshots

### program_snapshots

~~~text
tenant_id
project_id
snapshot_id
source_snapshot_digest
declared_schema_digest
compiler_version
created_at
~~~

### program_snapshot_aliases

~~~text
tenant_id
project_id
snapshot_id
release
git_revision
application_build
service_name
first_seen_at
last_seen_at
~~~

Source/schema bodies stay in object storage.

## Ingest and commitment

### ingest_sessions

~~~text
tenant_id
project_id
environment_id
session_id
producer_id
cell_id
lane_id
state
capture_policy_id
index_policy_id
durability_level
admitted_bytes
committed_bytes
created_at
expires_at
completed_at
~~~

### ingest_authorizations

~~~text
tenant_id
project_id
session_id
authorization_id
ledger_date
object_key
expected_bytes
expected_checksum
reserved_at
expires_at
consumed_at
~~~

### commit_receipts

~~~text
tenant_id
project_id
session_id
commit_id
receipt_id
manifest_set_digest
receipt_object_ref
receipt_checksum
signature_key_version
state
created_at
anchored_at
~~~

## Artifact ledger

### artifact_chunks

~~~text
ledger_date
tenant_id
project_id
environment_id
cell_id
lane_id
chunk_id
session_id
commit_id
source_artifact_id
source_generation
stream_id
stream_epoch
stream_kind
chunk_sequence
predecessor_digest
content_digest
envelope_digest
object_ref
object_checksum
manifest_hash
encoded_bytes
decoded_bytes
record_count
min_event_time
max_event_time
artifact_schema_version
decoder_support_state
integrity_state
committed_at
tombstoned_at
~~~

Chunk identity:

~~~text
(tenant_id, project_id, stream_id, stream_epoch, chunk_sequence)
~~~

**ledger_date is not part of identity.** It is a partition/key-routing attribute fixed at first authorization. Same identity + same immutable manifest hash is idempotent success; an immutable-field difference is conflict/quarantine.

The known inventory uses **object_ref** as shorthand. The migration must make it a typed immutable reference containing at least object key and provider version ID, or split those into explicit columns; length and checksum remain separately bound. The exact composite/column spelling is a freeze gate, but key + version binding is not optional. The same rule applies to receipt object references.

### stream_heads

~~~text
tenant_id
project_id
environment_id
cell_id
lane_id
stream_id
stream_epoch
ledger_date
previous_epoch
previous_epoch_root
contiguous_committed_through
contiguous_anchored_through
completion_state
final_sequence
created_at
rotated_at
~~~

The client durability watermark is the minimum of the two contiguous-through fields.

## Runs

### runs

~~~text
tenant_id
project_id
environment_id
run_id
boundary_id
root_process_id
root_engine_id
root_thread_id
root_call_id
program_snapshot_id
execution_state
structural_completeness
value_completeness
integrity_state
projection_state
retention_state
started_at
ended_at
state_version
~~~

### run_artifact_attachments

Table role is specified; literal columns are not.

### run_relationships

Known columns:

~~~text
parent_run_id
child_run_id
relation_kind
evidence_ref
~~~

Tenant/project/environment scope columns and keys still need explicit DDL.

### stream_completions

Table role is specified; literal columns are not.

## Projection workflow

### projection_outbox

~~~text
ledger_date
tenant_id
project_id
environment_id
cell_id
lane_id
outbox_id
chunk_id
projection_kind
generation
payload
created_at
claim_owner
claim_expires_at
next_attempt_at
attempts
published_at
last_error
~~~

The older shorthand “tenant/project/environment/cell/lane” expands to the five explicit scope columns above.

### projection_stream_checkpoints

~~~text
tenant_id
project_id
stream_id
stream_epoch
projection_kind
generation
next_sequence
lease_owner
lease_epoch
lease_expires_at
state_snapshot_ref
state_snapshot_sequence
state_snapshot_digest
blocked_state
updated_at
~~~

### projection_batches

~~~text
tenant_id
project_id
projection_batch_id
projection_commit_id
generation
physical_table
batch_manifest_ref
expected_unique_rows
expected_digest
state
verified_at
~~~

### projection_generations

~~~text
tenant_id
project_id
projection_kind
generation
schema_version
decoder_version
state
created_at
validated_at
activated_at
retire_after
~~~

### cell_backlog_counters

~~~text
cell_id
work_class
pending_chunks
pending_encoded_bytes
pending_estimated_records
oldest_pending_committed_at
incoming_bytes_ewma
counter_version
reconciled_at
~~~

## Known responsibilities without frozen tables

The design also requires storage for:

- routing regions/cells and deployment registry;
- capture/value-access/query policy versions;
- commitment compaction segments and membership intervals;
- audit;
- deletion/legal holds;
- saved SQL text;
- operation records for core reconstruct/reindex/export; and
- authentication/authorization material.

Literal tables/columns are not present in the source design. They are schema freeze gaps, not permission to improvise during implementation.

Durable background query jobs are deferred and do not require v1 operation tables beyond the core operations.

## Transaction invariants

### Commit

One short transaction:

1. validates/resolves exact authorization and scope;
2. inserts or idempotently resolves chunk identity;
3. rejects conflicting manifest hash;
4. creates projection requirements for active/building generations;
5. records a pending deterministic receipt;
6. advances only contiguous committed heads;
7. writes audit/accounting facts; and
8. writes an outbox row.

No object download/decode and no ClickHouse I/O occurs inside it.

### Projector

- No PostgreSQL transaction remains open during S3/ClickHouse work.
- Leases carry monotonic fence epochs.
- A stale worker cannot checkpoint.
- next_sequence advances only after analytical visibility verifies.
- Gaps become durable blocked state; SQS order is irrelevant.

### Receipt

The receipt object is anchored after commitment. Only anchored contiguous state is reclaimable by the producer.

## Ledger compaction

Hot chunk rows are not the forever ledger. A content-addressed manifest segment covers a verified contiguous range with ordered IDs/digests/object refs/manifest hashes/bytes and a previous/root digest.

Before dropping hot partitions, verify:

- every row covered exactly once;
- segment/checkpoint objects exist;
- no conflict, hold or deletion blocks it; and
- rollback/grace elapsed.

The exact segment registry and membership-interval tables are not frozen.

## Tenant isolation

- Enable and force RLS on tenant tables.
- Tenant-facing roles are non-owners without BYPASSRLS.
- Repositories require a scoped transaction with transaction-local tenant/project context.
- Cross-tenant background work uses a small audited set of SECURITY DEFINER routines owned by a non-login role, with PUBLIC revoked, pinned search_path, schema-qualified objects, validated cell/work class, timeouts and minimal returned columns.
- Test with deployed non-superuser roles.

## Connections and migrations

- SQLx, bounded direct RDS pools, no concurrent ORM stack.
- Terraform rejects aggregate role pools above 70% of max_connections including reserve.
- Forward-only production migrations.
- Expand/backfill/contract across releases; long backfills are operations.
- One isolated migration task.
- Immutable migration files after merge.
- PG and ClickHouse migration credentials remain distinct.

Expected source layout:

~~~text
db/postgres/migrations/
crates/<storage>/queries/
.sqlx/
~~~

It does not exist yet.

## DDL freeze checklist

- Fill every placeholder table.
- Assign concrete types/nullability/defaults.
- Define PK/unique/FK constraints with tenant/project scope.
- Define partitions/indexes.
- Freeze state lookup constraints and transitions.
- Define receipt/segment/audit/deletion tables.
- Write forced-RLS policies and attack tests.
- Specify PITR/restore plus receipt import.
- Commit SQLx offline metadata.
