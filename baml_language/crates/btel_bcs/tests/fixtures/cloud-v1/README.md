# Cloud v1 contract fixtures

Proposed client contract examples for sharing with BCS. **Not yet approved by
BCS.** These are independent scenarios, not successive requests in one recording.
They deliberately reuse a fixed recording ID and file sequence.

## Upload cases

| Directory | Candidates | Expected physical PUTs |
| --- | --- | --- |
| `inline` | Scalar 10 inline | Recording with one CAS object |
| `batch` | Scalars 10, 11 in one batch | Recording without CAS; batch with both objects in order |
| `standalone` | Scalar 10 standalone | Recording without CAS; one standalone CAS envelope |
| `all-skip` | Scalars 10 through 14 already available | Recording without CAS; all three CAS-only targets removed |
| `mixed` | Inline 10, batch 11, skip 12, standalone 13, skip 14 | Recording with 10; batch with 11; standalone 13 |
| `expired` | Scalar 10 inline | None: target URL expired, despite a future plan expiration |
| `recording-only` | Zero candidates | Recording without CAS |

The mixed case removes candidate 2 from a nonempty batch and removes the separate
batch target containing only candidate 4. All four dispositions coexist.
`all-skip` also prunes the inline member but must still upload the recording.

Each case contains:

- `prepare.json`: exact JSON value expected from the prepare POST. Object-key
  order and whitespace are not significant; array order and all fields are.
- `response.json`: fixed authored server JSON, served with HTTP 200. It is never
  generated from the incoming client request.
- `puts.json`: prepare path and headers, and each expected PUT's upload ID, path,
  query, headers, body filename, byte length, and lowercase SHA-256.
- `*.hex`: exact protobuf `CloudUploadEnvelope` bytes as lowercase hex text.
  Lengths and SHA-256s refer to decoded bytes, not the hex text or trailing LF.

`expired/puts.json` has no uploads. Its `forbidden_body` describes the envelope
that must never reach HTTP. The test expects `DeliveryError::Expired` and zero
PUTs. Normal expirations are fixed at 4102444800000 (2100-01-01 UTC); the expired
target is fixed at 1. No test sleeps until expiration or substitutes the clock.

Recording and CAS targets may upload independently. Tests match PUTs by path and
upload ID, never arrival order. Each admitted snapshot pool has zero owners after
delivery drains, including expiration and all-skip.

## Sources and provenance

`sources/manifest.json` freezes the initial existing encodings, not invented CAS
bytes: five `SnapshotPool` scalar integer values 10 through 14, their existing
SnapshotIds, CAS v1 blobs, lengths, and SHA-256s. `golden_uploads.rs` constructs
these structured owners again and compares the exact source encodings.

Recordings were initially sealed by the real `RecordingBuilder`, with recording
ID `[7; 16]`, sequence 1, one aggregate count, and zero, one, two, or five
`FunctionSpanCompletionOk` value captures. Each recording references exactly its
case's offered snapshot IDs, including IDs the server skips. Scalar snapshots are
completion values, not function-argument captures. The zero-candidate recording
contains no snapshot references. These are recording fragments: definition and
thread/clock metadata need not be in the same file.

The integration-test binary reserves telemetry IDs once and asserts exact numeric
IDs 1 through 6. Thread/parent ID is 1, completion IDs are 2 through 6, call-path
ID is 7, and entered/exited ticks are 100/200. No machine clock epoch, random span
ID, or capture-time timestamp is embedded in the source bytes. The private
`TelemetryId` API is respected; no unsafe IDs are manufactured.

Packaging is explicitly supplied to `BcsDelivery`, not selected using the
publisher's default size thresholds. Standalone proposals therefore carry
`LARGE` even for these small test scalars. That classification is a packaging
policy hint, not an exact blob length. This keeps examples small without inventing
an alternate CAS encoding.

Initial prepare metadata was authored from the frozen source lengths/IDs/digests
and explicit proposed membership. Server plans and dispositions were authored
separately as fixed tables. Expected envelope bytes were generated using vendored
`protoc-bin-vendored` crate 3.2.0 (compiler `libprotoc 31.1`,
`protoc --encode=btel.cloud.v1.CloudUploadEnvelope`)
against `proto/cloud.proto`, supplying the frozen recording/CAS bytes and authored
plan/upload IDs. They were also decoded using that schema. No production cloud
envelope encoder or incoming HTTP request produced the expected envelopes.

Tests independently check envelope version, IDs, ordered membership, recording
presence and exact source bytes, CAS format version and exact blob bytes, each
blob SHA-256, and whole-envelope length/SHA-256. Corruption sensitivity tests
change a blob byte or remove a member and require a mismatch against the frozen
body. Updating an encoder must not automatically rewrite these fixtures.

## Allowed substitutions and headers

Only `https://uploads.invalid` is replaced by a wiremock localhost origin.
Paths and queries are preserved. Bearer strings and capabilities are inert
fixture literals, not credentials.

Before comparing prepare JSON, the test validates that the actual producer
session is a v4 UUID and its sequence is positive, then substitutes only
`producer_session_id` and `liveness_sequence` with the fixed fixture values.
The prepare is observed before calling `finish`, so `state` must be exactly
`RUNNING`; it is not normalized. No candidate, membership, plan/upload ID,
digest, or body bytes are normalized.

Prepare requires the fixed bearer, JSON content type, and
`07070707070707070707070707070707:1` idempotency key. PUTs have no BCS authorization
or idempotency header. Required headers have exactly one value; content length
equals the fixed body length. The inline response also requires
`x-amz-content-sha256: UNSIGNED-PAYLOAD`, which can be signed before serialization
without binding an unknown whole-body digest. Other targets exercise the default
protobuf content type.

## Verification

From `baml_language`:

```sh
cargo +1.98.0 test -p btel_bcs --test golden_uploads
```

To inspect current source encodings without overwriting any golden:

```sh
cargo +1.98.0 test -p btel_bcs --test golden_uploads \
  print_source_encodings -- --ignored --nocapture
```

For independent protobuf inspection, from `crates/btel_bcs`, set `PROTOC` to the
vendored binary for your platform:

```sh
xxd -r -p tests/fixtures/cloud-v1/mixed/recording.hex |
  "$PROTOC" --decode=btel.cloud.v1.CloudUploadEnvelope proto/cloud.proto
```

Decode then re-encode an envelope with the same schema to check its outer wire
encoding. Embedded recording/CAS payloads must remain opaque exact bytes; the
recording encoder may intentionally use nonminimal container-length varints.

The adjacent `heartbeat` fixtures cover sender states and duplicate/out-of-order
observations separately from upload correctness.
