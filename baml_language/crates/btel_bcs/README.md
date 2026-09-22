# Btel cloud delivery

This crate implements the runtime side of a proposed BCS direct-upload contract.
It does not imply that the BCS server already supports this protocol.

## Transport

The runtime posts JSON to
`/v1/recordings/{recording_id}/uploads:prepare`. Recording IDs and digests are
lowercase hexadecimal. Snapshot IDs are the existing 16-byte XXH3-128 identities,
not the SHA-256 of their encoded blobs. `recording.sha256` covers the exact sealed
recording bytes.

The `Idempotency-Key` is `{recording_id}:{recording_file_sequence}`. The server
must bind it to the original request contents and return the same immutable plan
on retry; conflicting immutable contents must fail. Process liveness fields are
fresh on each attempt and excluded from that idempotency comparison. There is
no URL-renewal operation.

`wire.rs` is the JSON schema's source of truth. Proposed and returned targets use
`kind`: `recording`, `cas_batch`, or `cas_object`, with ordered
`candidate_indices`. Each response disposition uses a nested `disposition` with
`kind`: `inline_with_recording`, `member_of_batch`, `separate_object`, or
`already_available`. The first three include `upload_id`. Expiration fields use
Unix milliseconds.

BCS must preserve the client's placement for missing snapshots, remove available
members, and omit empty CAS-only targets. It must return exactly one recording
target even if every snapshot is already available. Availability means validated
content within the authorized organization, not merely an existing S3 object.

Each surviving target is an ordinary HTTP PUT containing a
`btel.cloud.v1.CloudUploadEnvelope` from `proto/cloud.proto`, format version 1.
It embeds the exact recording bytes and the canonical CAS v1 blobs. The blob
SHA-256 is an integrity check separate from the snapshot identity.

Only prepare requests receive the BCS bearer credential. Upload requests use the
returned URL and required headers; redirects are not followed. URLs and headers
are capabilities and must not be logged.

The supported S3 signing policy uses an unsigned payload, optionally with the
constant required header `x-amz-content-sha256: UNSIGNED-PAYLOAD`. Exact payload
checksums cannot be presigned before snapshot serialization. Other S3 checksum
modes are rejected rather than guessed; BCS must agree to this policy. Envelope
and blob integrity still require server-side validation.

PUT success is the delivery acknowledgement, not proof of server ingestion or
query availability. Retries reuse the same body and URL. BCS must issue URLs
whose lifetimes cover the bounded retry window. Expiration drops that payload;
it does not stop later delivery.

## Scope

There is no durable spool, recovery after process exit, or URL renewal.
Orderly shutdown drains admitted delivery work.
Cloud contract tests use local HTTP servers and require neither BCS nor AWS;
they do not establish interoperability with a deployed server.

## Payload failures

Retryable transport and HTTP failures get one initial attempt plus three retries
by default. Invalid plans, expired URLs, and nonretryable HTTP errors are dropped
without futile retries. A failed prepare discards its group; a failed upload
discards only that physical target. Other targets and later windows continue.
Temporary admission saturation still applies bounded backpressure.

There is no circuit breaker, cooldown, probe loop, or durable retry spool.
Only fatal worker failures or explicit cancellation disable cloud recording.
`BexEngine::telemetry_delivery_loss_count` counts discarded groups/targets, not
events or retry attempts. `telemetry_result` retains an error after loss even
when later uploads succeed. Delivered recording sequences may have gaps.

This deliberately replaces the original cloud contract's permanent-disable and
offer-once policies. BCS must tolerate missing recordings/captures and repeated
CAS offers; only BCS decides whether content is already available.

## Ownership and limits

The processor statically dispatches to `CloudPublisher`. It reuses
`RecordingBuilder` for encoding and hands owned groups to a dedicated delivery
thread; event callbacks never perform HTTP.

The publisher accumulates recording events and structured snapshots across
processor batches. A window seals at a record boundary when it reaches the
recording-byte target, 16 distinct snapshots, or 4 MiB of retained snapshot
storage. A single snapshot may exceed a soft byte target. The non-sliding timer
starts at the first event, using `RecordingConfig::flush_interval_duration`;
idle expiry and explicit shutdown also seal pending data.

Sealed windows are staged until processor input chunks have been recycled.
The open window and staged windows together retain at most 32 snapshot owners
and 8 MiB of snapshot storage by default. Staged recording files are separately
bounded by delivery's plan/recording-byte limits. Construction validates that
publisher and delivery owner limits leave headroom in the minimum VM pool.
No network I/O runs on the processor. Admission waits occur only after recycling.

Delivery separately limits pending owners, plans, recording bytes, and CAS bytes.
It releases owner-slot reservations after serialization releases the snapshots,
not after their PUTs finish. Immutable upload bodies remain byte-budgeted until
delivery completes. Delivery admission waits for temporary plan, byte, or
owner-slot saturation. Failure, closure, and released capacity wake waiting
submitters. An individual group that can never fit fails before waiting.
Callers must recycle input chunks before entering admission.

Cloud processing moves one chunk into reusable owned storage and recycles its
source allocation before incremental processing and handoff. Span and timing
storage each reserve the transport's chunk capacity; only one contains live
records at a time. Captures are moved, not cloned, and retain their existing
VM snapshot-pool owners until processed or dropped. Local/no-sink processing
keeps its original borrowed path without these allocations.

This permits large chunks to make progress through bounded publisher windows
and delivery backpressure. Snapshot accounting is cached between preflight and
retention rather than scanning each new snapshot twice. Dropped payloads remain
observable without stopping later telemetry.

Snapshot IDs use bounded recent deduplication. Loss makes affected content
eligible to be offered again; reaching the history limit evicts old entries.
Placement thresholds use retained-memory estimates,
not exact encoded lengths; output encoding has independent hard limits.

Unacknowledged recording metadata is replayed in later files, without replaying
spans or aggregates. Its journal is bounded; prolonged outages can evict old
metadata and leave references unresolved. `telemetry_metadata_replay_evictions`
reports these evictions separately from confirmed payload loss, since an
in-flight copy may still succeed. This is best-effort recovery, not a guarantee
of complete history after arbitrary outages.

Snapshot accounting conservatively charges arena capacities and shared backing.
It excludes the preexisting shared pool and allocator metadata. Bigint values
and bigint type literals currently have no safe retained-capacity bound through
`num-bigint`'s public API. Unsupported payloads are discarded instead of
silently violating the budget. This does not change local recording or BAML
execution.

## Optional liveness

Every prepare attempt carries a process-stable random `producer_session_id`,
checked process-wide `liveness_sequence`, and producer `state`. BCS can return
`heartbeat: { interval_ms, staleness_threshold_ms }`. A separate worker sends
`POST /v1/recordings/{recording_id}/heartbeat` when the configured inactivity
interval expires. Only completed successful BCS POSTs reset that deadline;
S3 PUTs and failed POSTs do not.

Heartbeat errors are advisory and observable through
`BexEngine::telemetry_heartbeat_error`, without failing delivery or execution.
Closing delivery marks it `DRAINING`; that state appears on subsequent
control traffic without forcing an early heartbeat. Quick drains may finish
before a heartbeat is due. Shutdown cancels in-flight heartbeats rather than
waiting for the interval.

## Performance probes

From `baml_language`, run the opt-in per-call diagnostic:

```sh
cargo test --release -p bex_engine --test telemetry_performance \
  -- --ignored --exact telemetry_performance --nocapture
```

Each scenario runs in a fresh process. The default is five cyclically balanced
trials, with 20 paced warmup calls and 500 measured calls per trial.
`BTEL_PERF_TRIALS` and `BTEL_PERF_ITERATIONS` override those counts. Compilation,
engine construction, argument/context preparation, and warmup are outside call
latency; shutdown is timed separately.

The JSON output includes latency percentiles, throughput, delivery status, and
request counters before measurement, after measurement, and after shutdown.
Do not interpret post-failure latency as successful telemetry performance, or
uploads deferred until shutdown as concurrent-upload overhead. Paced calls are
closed-loop measurements, not an independent arrival stream.

This per-call probe's mock server shares the process. For whole-runtime CPU/RSS
measurement with a separate server process, use the
[native resource workload runner](../bex_engine/examples/resource_workloads/README.md).
Both probes use loopback HTTP, not deployed BCS/S3. Keep machine-specific results
and generated reports under ignored `target/` output rather than in the source
tree.

## Golden contract fixtures

The versioned fixture set lives in [`tests/fixtures/cloud-v1`](tests/fixtures/cloud-v1).
It fixes prepare JSON, server plans, canonical recording/CAS sources, upload
protobuf bytes, hashes, headers, expiry behavior, and heartbeat wire examples.
The HTTP golden tests serve checked-in responses rather than manufacturing a
plan from the client's request.

```sh
cargo test -p btel_bcs --test golden_uploads --test golden_heartbeat
```

JSON comparisons preserve every protocol field while ignoring object key order
and whitespace. Protobuf upload bodies are compared byte-for-byte. Only documented
test-local origins and nondeterministic liveness identity/sequence fields may be
substituted; membership, ordering, versions, digests, and payload bytes may not.

These vectors are ready for cross-repository contract review, not proof that BCS
has adopted the protocol. Updating a fixture is an intentional wire-contract
change requiring review, not an automatic snapshot-accept operation.
