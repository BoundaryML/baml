# Btel cloud delivery

Cloud contracts and single-attempt HTTP calls live in `bcs_api`. This crate owns recording assembly, bounded delivery, retries, heartbeat scheduling and flush.

This crate implements the runtime side of a proposed BCS direct-upload contract.
It does not imply that the BCS server already supports this protocol.

## Automatic function observation

A function requesting auto (`null` / no explicit policy) inherits
`AutoTelemetryLevel`. The engine reads `BAML_TELEMETRY` once at startup:

- `low`: auto functions are unobserved.
- `medium` (default): ordinary auto functions produce timing aggregates; auto LLM
  functions produce spans with their default input/output/error captures.
- `high`: supported auto functions produce spans, without extra value capture.
- `off`: global kill switch; no telemetry resources are created.

Explicit function policies take precedence over every automatic level. In
particular, `high` does not promote an explicitly configured timing-only function.
Native/unsupported functions remain unobserved. Per-function auto is distinct
from the engine level: the former engine-level spelling `auto` is replaced by
`medium`. There is not yet a public BAML policy setter accepting `null`.

## Transport

`baml run`, `baml test`, and native SDK runtimes select cloud recording from
`BOUNDARY_API_KEY` or a saved `baml auth login` session. `BOUNDARY_API_URL`
overrides `[boundary].api_url` and defaults to `https://api.prod.bcs.boundaryml.com`.
User sessions write to the user's personal environment; API keys enforce their
provisioned ingestion scope. `BOUNDARY_PROJECT` overrides `[boundary].project`.

Without cloud credentials, hosts use their local recording defaults. Explicit
`BOUNDARY_API_KEY=local` selects local recording and bypasses Boundary endpoint
validation. Otherwise an invalid Boundary URL cancels execution with an actionable
configuration diagnostic. Initial authorization rejection also cancels execution;
later revocation disables cloud recording and lets execution continue.
`BAML_TELEMETRY=off` disables recording. Cloud recordings omit process arguments.
Environment reads live in `bex_engine::TelemetryRecording`, not this transport crate.

Construct `DeliveryConfig::new(endpoint.parse()?)` with an explicit BCS endpoint.
The config retains a parsed `reqwest::Url`; startup still enforces HTTPS and
rejects credentials, queries, and fragments in that base URL.

Plain HTTP is accepted only for the loopback hosts `localhost`, `127.0.0.1`, and
`[::1]`, for local development against a local data plane. Upload targets may
use HTTP only to a loopback host and only when the base URL is itself loopback
HTTP; an HTTPS base never sends uploads over HTTP.

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

`bcs_api::wire` is the JSON schema's source of truth. Proposed and returned targets use
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
`btel.cloud.v1.CloudUploadEnvelope` from `bcs_api/proto/cloud.proto`, format version 1.
It embeds the exact recording bytes and the canonical CAS v3 blobs. The blob
SHA-256 is an integrity check separate from the snapshot identity. The body is
sent with a fixed `Content-Length`, never chunk-encoded.

Snapshot/hash format v3 keeps BEP-075's attribute-free type representation and
numbers each blob's objects in first-reference order, so a blob has exactly one
encoding. A v3 blob may name child blobs by ID, and a capture may be several
blobs. Each candidate is one blob: a capture's blobs are offered children
first, and a capture with more new blobs than one plan takes is spread over
several plans, in files that may carry no events. A `snapshot_id` names a
blob. BCS must decode version 3; older CAS objects remain separate namespaces
and are not reused as v3 content. The upload envelope remains v1.

Only prepare and heartbeat requests receive the BCS bearer credential. Upload
requests use the returned URL and required headers; redirects are not followed.
URLs and headers are capabilities and must not be logged.

The supported S3 signing policy uses an unsigned payload, optionally with the
constant required header `x-amz-content-sha256: UNSIGNED-PAYLOAD`. Exact payload
checksums cannot be presigned before snapshot serialization. Other S3 checksum
modes are rejected rather than guessed; BCS must agree to this policy. Envelope
and blob integrity still require server-side validation.

PUT success is the delivery acknowledgement, not proof of server ingestion or
query availability. Retries reuse the same body and URL. Expiration drops that
payload; it does not stop later delivery.

A PUT is given `request_timeout` (10 s) plus its body's length at
`min_upload_bytes_per_second` (1 MiB/s): a large body is not lost for taking
long, only for uploading slower than that. A body's retry window is every
attempt at that timeout and the delays between them: 40.6 s for a small body,
about 73 s for 8 MiB, about 69 minutes for 1 GiB. Recordings upload one at a
time and CAS bodies `max_cas_uploads` (2) at a time, each when its turn comes,
so BCS must issue each URL a lifetime that covers the windows of every body of
its kind admitted before it, counted as if they went one by one. A URL that
does not is refused before anything is sent to it. The prepare request does
not carry blob lengths, only `size_class: LARGE` for a blob uploaded on its
own.

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
`btel_recorder::RecordingBuilder` for encoding and hands owned groups to a dedicated delivery
thread; event callbacks never perform HTTP.

`CloudPublisher` submits through `BcsDeliveryHandle`; the engine retains
`BcsDelivery` to drain and join the worker. Local recording has the same ownership
split through `LocalDeliveryHandle` and `LocalDelivery` in `btel_file`. These are
implementation details, not a shared delivery interface required by `Publisher`.

`btel_processor` defines the shared `Publisher` interface. `CloudPublisherConfig`
configures cloud-specific batching and placement; `RecordingConfig` is shared
with local recording, and `DeliveryConfig` controls cloud transport.

The publisher accumulates recording events and queues the blobs of captured
values across processor batches; a blob offered recently is not queued again,
and a capture with nothing new to offer is released at once. A window seals at
a record boundary when it reaches the recording-byte target, 16 queued blobs,
or 4 MiB of retained capture storage. A single capture may exceed a soft
target. Each sealed file takes the head of the queue: at most one plan's worth
of blobs, cut short where the bytes their upload bodies buffer pass a quarter
of delivery's CAS reservation, and always at least one blob. While 16 or more blobs still wait,
or more than one file takes, further files are sealed to carry them, and at
the end the last file takes what remains. The non-sliding timer starts at the
first event, using `RecordingConfig::flush_interval_duration`, and starts
again when a sealed file leaves blobs queued. When it expires, or on an
explicit flush, the open file is sealed and further files carry every blob
still queued, so the tail of a large capture waits no longer than an event
does. A capture queued before the first event (the project's sources) waits
for that event's file. Explicit shutdown also seals pending data.

Nothing is refused for its size. A blob is uploaded however large it is: the
only size a capture is cut for is `btel_settings::snapshot::MAX_LEAF_BYTES`,
when it is made. An upload body is written field by field, so a blob is
buffered once, beside its capture. A blob that is one string of 2 KiB or more
is not buffered at all: its content is sent from the memory that already holds
it, and that string is let go when its upload ends. The window cut and the
CAS reservation count only what is buffered.

Sealed windows are staged until processor input chunks have been recycled.
The queue and staged windows together retain at most 32 captures and 8 MiB of
capture storage by default. A blob that is one string holds its own content:
it is let go as soon as the server reports it stored, or else when its upload
ends (a short one, once it has been copied into its body), whatever becomes of
the rest of its capture. The rest of a capture is
held until the last blob written from it leaves, and a capture that is one
string is not held at all.
Before a capture that does not fit, the publisher sends what it holds,
waiting on delivery's admission, and a capture larger than the whole byte
budget is then held alone.
Staged recording files are separately bounded by delivery's plan/recording-byte
limits. Construction validates that publisher and delivery capture limits leave
headroom in the minimum VM pool. No network I/O runs on the processor.
Admission waits occur only after recycling.

Delivery separately limits pending captures, plans, recording bytes, and CAS
bytes; a capture spanning plans counts once per plan. A plan is pending until
its recording is uploaded. Its captures, and the storage slots they hold, are
given back sooner, once its upload bodies are built. Each CAS body is uploaded
on its own, whatever becomes of the plan it came with, and holds its part of
the CAS reservation (what it buffers, and the strings it sends from where they
are held) until its upload ends. So one long upload keeps neither later plans
nor other bodies waiting.

Delivery admission waits for temporary plan, byte, or owner-slot saturation.
Failure, closure, and released capacity wake waiting submitters. A group whose
recording can never fit fails before waiting. One whose captures and CAS
bodies total more than the whole CAS reservation is never refused: it is
admitted beside the reservation, one such group at a time, so the most held is
the reservation and one group. Groups that fit are admitted meanwhile; only
another oversize group waits.
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

Blob IDs use bounded recent deduplication. Loss makes affected content
eligible to be offered again; reaching the history limit evicts old entries.
Placement thresholds use each blob's exact encoded length. A recording body
has a hard limit; a CAS body is as long as its blobs.

Unacknowledged recording metadata is replayed in later files, without replaying
spans or aggregates. Its journal is bounded; prolonged outages can evict old
metadata and leave references unresolved. `telemetry_metadata_replay_evictions`
reports these evictions separately from confirmed payload loss, since an
in-flight copy may still succeed. This is best-effort recovery, not a guarantee
of complete history after arbitrary outages.

Snapshot accounting conservatively charges arena capacities and shared backing.
It excludes the preexisting shared pool and allocator metadata. Bigint values
and bigint type literals are charged their limbs: `num-bigint`'s public API
does not show spare capacity, so for them the figure is a close estimate, not
a bound. This does not change local recording or BAML execution.

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
