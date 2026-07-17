# C# stream and media actual-ABI evidence

This record covers verification gates B8 and B9 against Current Canary. It
records the exact passing Linux x64 actual-ABI stream and media executions.

## Target and fixture

- Target baseline: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0` on
  `paulo/csharp-bridge`.
- Audit host: Linux x64; .NET SDK `10.0.110`; runtime `10.0.10`; C# 14 /
  `net10.0`.
- Native library: fresh isolated current-source `release-bridge-cffi`
  Linux x64 artifact, 24,661,376 bytes, SHA-256
  `dc8d399dbfaa14327be3eed25a52fa8661333ce211ca2d5a38fe0a833a323432`;
  product version `0.15.0`.
- Probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe`.
  It uses one source-generated `baml_get_api_v1` import, the validated V1 API
  table, the pinned internal Protobuf generation contract, static unmanaged
  callbacks, copied callback bytes, and explicit buffer/handle release.
- The probe has separate `media` and `stream` process modes because the native
  runtime accepts one BAML program per process. Stream mode also separates the
  replay server/runtime and stream consumer into child processes so the
  consumer receives `BAML_REPLAY_BASE_URL` before its native runtime snapshots
  process environment.

Probe source hashes after the passing B8/B9 runs:

| File | SHA-256 |
| --- | --- |
| `Baml.Bridge.StreamMediaAbiProbe.csproj` | `e8e9f4ed83cb55d7241039805d59cae07cf893a34499626af360dc47d9e4aae7` |
| `NativeBridge.cs` | `1d3c4633789bc732da33bdbf6dcafba4b8485b8f87faa7a45023e81699d663bf` |
| `Program.cs` | `b13e15bf6628ab93210d3fc1a353f97ae24d4c039f9c97d74cc565d16c36f58d` |
| `README.md` | `850c1cc78539d3fbc7c810a079187b222e9b08a27aed46fbcd1b0b03c7465567` |
| `../Baml.NativeProbeMode.targets` | `01445658f4cce7e9531f6c1154e8ef1924974660b1f8f86da0b035da680c0776` |

The project enables the .NET trim analyzer and declares itself trimmable; the
ordinary Release build and the self-contained linked publish both remain
warning-free with linker warnings treated as errors. The passing media output
is unchanged. Its trimmed media/stream execution is also wired into the manual
external-run handoff in `TASK/csharp-entry-gates-handoff.md`.

The repository-owned ignored emitter tests in
`baml_language/sdk_tests/harness_setup/src/lib.rs` load a canonical fixture
through `ProjectDatabase`, reject diagnostics, serialize the current compiled
program with Borsh, and write only to an explicitly supplied output path:

```shell
cd baml_language
BAML_CSHARP_MEDIA_PROBE_BYTECODE=/tmp/baml-csharp-b9-type-shapes.bytecode \
  env RUSTC_WRAPPER= cargo test -p sdk_test_harness_setup \
  csharp_abi_probe_tests::emit_type_shapes_bytecode \
  --lib -- --ignored --exact --nocapture

BAML_CSHARP_STREAM_PROBE_BYTECODE=/tmp/baml-csharp-b8-llm-functions.bytecode \
  env RUSTC_WRAPPER= cargo test -p sdk_test_harness_setup \
  csharp_abi_probe_tests::emit_llm_functions_bytecode \
  --lib -- --ignored --exact --nocapture
```

Both tests passed. The media bytecode is 752,844 bytes with SHA-256
`c113b497a01afab5add4b3a11ff01bd7eaafa9d8608eb48237da5aaef22f69b3`.
The stream bytecode is 702,685 bytes with SHA-256
`e10764d644d29c566598d2744f028bd3b5f3cdc5d00a46de7078ee18b94079c0`.

The Release probe build used the already isolated exact package feed:

```shell
env NUGET_PACKAGES=/tmp/baml-csharp-protobuf-nuget \
  dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe/Baml.Bridge.StreamMediaAbiProbe.csproj \
  --configuration Release --nologo \
  -p:NuGetAudit=false \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlProtocolProbeFeed=/tmp/baml-csharp-protobuf-feed-20260717
```

Result: zero warnings and zero errors.

## B9: media restoration passes locally

The actual process command was:

```shell
env NUGET_PACKAGES=/tmp/baml-csharp-protobuf-nuget \
  dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe/Baml.Bridge.StreamMediaAbiProbe.csproj \
  --configuration Release --no-build --no-restore \
  -p:BamlNativeProbeMode=Direct -- \
  /root/baml-current-native-evidence.NGfRFQ/cargo-shipping/x86_64-unknown-linux-gnu/release-bridge-cffi/libbridge_cffi.so \
  0.15.0 media /tmp/baml-csharp-b9-type-shapes.bytecode
```

It executed 17 canonical `type_shapes` calls and reported:

```text
media_kinds=url_base64_file_4x
media_actual_envelope=handle_eagerly_restored
media_file=eager_owned_bytes
media_decode_failure=handle_released
media_inline_protocol=url_base64_file
media_handles_restored=17
product_version=0.15.0
bytecode_bytes=752844
native_callbacks=17
owned_buffers_released=79
```

The current actual envelope for each media result is:

```text
class_value(
  name = "baml.media.Image|Audio|Pdf|Video",
  fields["_data"] = handle_value(ADT_MEDIA_*))
```

It is not a bare `handle_value` and is not normally the protocol's inline
`media_value`. The managed decoder must validate the nominal class and the
inner handle type, read exactly one URL/base64/file representation plus MIME
type through the V1 table, copy/decode bytes immediately, and release the
native handle in a `finally` path. Managed encoding performs the inverse:
create the ephemeral native media handle, clone the reference transferred to
the wire, and wrap that clone in the nominal class's `_data` field.

For image, audio, PDF, and video separately, the probe proves:

- BAML-created URL output preserves Unicode URL text and MIME type.
- Host-created URL values survive a BAML round trip without fetching.
- Host-created base64 values restore identical owned bytes and MIME type.
- Host-created file values restore identical owned bytes and MIME type; the
  source file is deleted before the restored value is inspected, proving eager
  ownership rather than a deferred path.
- The managed-owned original handle remains readable after its transferred
  clone is consumed.
- An intentional class/type mismatch releases the result handle before
  surfacing the decode failure.
- The durable/inline protocol form independently covers URL, base64, and file,
  including eager file copying.

Conclusion: Q18 needs no protocol amendment. B9 is `passed locally`; the final
public immutable media implementation and the cross-RID product suite remain
implementation work.

## B8: content-identified pull design and execution pass locally

Current Canary exposes no separate streaming callback ABI.
`user.lorem.stream_e2e_extract$stream` returns a typed
`ADT_TAGGED_HEAP_HANDLE` for `baml.llm.Stream<TPartial,TFinal>`, and
`baml.llm.Stream.next` / `baml.llm.Stream.final` are ordinary calls. The
repository probe therefore implements bounded delivery with no queue: it
dispatches exactly one awaited `next` per managed `MoveNextAsync`. A deliberately
slow consumer checks one callback per demand and zero unsolicited callbacks
while idle.

The compiled stream mode also covers:

- cold factory start and exact single initialization;
- descriptor validation for handle type 14 and class FQN
  `baml.llm.Stream`;
- exact pull-union identity:
  `(string | null) | baml.stream.StreamFinished`, in that order, with the
  selected option name matched to the payload and exact terminal nominal
  class shape;
- two positive and seven fail-closed decoder vectors covering a typed partial,
  typed terminal, bare payload, changed descriptor, missing and unknown
  selected arm, both selected-arm/payload contradictions, and a wrong terminal
  nominal class;
- one or more nonnull pulls through the checked-in canonical SSE recording;
  the initial prefix may be empty, every later partial must be a strict ordinal
  extension, and the ordered incremental suffixes must reconstruct the exact
  canonical final rather than merely remain mutually consistent;
- exact final semantic identity in both drained and final-only paths:
  789 UTF-8 bytes with SHA-256
  `2e950ddbdb0c2e12f64c09bc6e4a72f687367894cdea17d632529fd6719d2ef2`;
- concurrent final callers and one cached final task for sixteen managed
  waiters;
- final-only draining without exposing partials;
- per-wait cancellation that does not cancel the shared final operation;
- pre-canceled native pull and early stream-handle release;
- callback-registry bounds and replay-server shutdown.

The checked-in recording is
`baml_language/sdk_tests/fixtures/llm_functions/recordings/replay_extract_string.snap.sse`,
39,050 bytes, SHA-256
`f16453f4198c3215ea7f1f0da0793f6453d64faaae6fcf19603a286a0444f7e9`.
The final direct-mode restore, build, and execution used a fresh package cache:

```shell
env -u BAML_NATIVE_PROBE_FEED \
  NUGET_PACKAGES=/tmp/baml-b8-direct-final-nuget \
  dotnet restore \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe/Baml.Bridge.StreamMediaAbiProbe.csproj \
  -p:NuGetAudit=false \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlProtocolProbeFeed=/tmp/baml-csharp-protobuf-feed-20260717

env -u BAML_NATIVE_PROBE_FEED \
  NUGET_PACKAGES=/tmp/baml-b8-direct-final-nuget \
  dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe/Baml.Bridge.StreamMediaAbiProbe.csproj \
  --configuration Release --no-restore --nologo \
  -p:NuGetAudit=false \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlProtocolProbeFeed=/tmp/baml-csharp-protobuf-feed-20260717

env -u BAML_NATIVE_PROBE_FEED \
  NUGET_PACKAGES=/tmp/baml-b8-direct-final-nuget \
  dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.StreamMediaAbiProbe/Baml.Bridge.StreamMediaAbiProbe.csproj \
  --configuration Release --no-build --no-restore \
  -p:BamlNativeProbeMode=Direct -- \
  /root/baml-current-native-evidence.NGfRFQ/cargo-shipping/x86_64-unknown-linux-gnu/release-bridge-cffi/libbridge_cffi.so \
  0.15.0 stream /tmp/baml-csharp-b8-llm-functions.bytecode \
  /root/dev/baml/baml_language/sdk_tests/fixtures/llm_functions/recordings/replay_extract_string.snap.sse
```

The build completed with zero warnings and errors. The direct execution passed
and reported:

```text
stream_union_metadata=exact_2_positive_7_negative
stream_partials=20
stream_partial_order=initial_prefix_then_strict_extensions_exact_canonical_final
stream_content_utf8_bytes=789
stream_content_sha256=2e950ddbdb0c2e12f64c09bc6e4a72f687367894cdea17d632529fd6719d2ef2
stream_pull=one_demand_one_completion
stream_idle=zero_unsolicited_completions
stream_cold_start=exactly_once
stream_final=multi_waiter_and_final_only
stream_wait_token=wait_only
stream_precancel_and_release=exact
stream_max_pending_calls=7
product_version=0.15.0
bytecode_bytes=702685
native_callbacks=34
owned_buffers_released=3
```

The exact `stream_partials=20` line above is an observation, not a public
boundary contract. During repair, identical direct runs against the same
recording produced 19 and 20 partials with different prefix lengths; another
run began with an empty prefix. All converged to the same exact final identity.
The strengthened assertion therefore retains the strongest stable ordered
identity—initial prefix, strict later extensions, lossless ordered-delta
reconstruction, and exact final bytes—without freezing parser scheduling or
provider chunk boundaries.

The final exact-package trim reproduction used the normalized local evidence
package
`Baml.Bridge.MultiRidPackageProbe.0.0.0-b4.nupkg`, SHA-256
`61eda292f7a5dab4565f38cb679feb5046c18b18449e517c9d7c34b074b7ab72`.
It restored into a fresh cache with
`BamlNativeProbeMode=Package`, published `linux-x64` self-contained with
`PublishTrimmed=true`, `TrimMode=link`, `TrimmerSingleWarn=false`, and
`ILLinkTreatWarningsAsErrors=true`, and completed with no trim warnings or
errors. A byte-for-byte `cmp` proved the restored package was the exact feed
input, and the publish directory contained exactly one `libbridge_cffi.so`.
The trimmed executable then ran with `package-default` and reported the same
metadata/content lines and complete passing stream summary shown above. Its
apphost was 78,256 bytes, SHA-256
`dd01cb382df0103cf066972b496813f736bd7d4fbc705a771216ee121f8dfb1f`;
its selected native asset retained SHA-256
`cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`.

The top-level process starts a bounded replay-server child and waits for its
selected localhost endpoint. It then starts a separate stream-consumer child
with `BAML_REPLAY_BASE_URL` and its test API key present in
`ProcessStartInfo.Environment` before process creation. This ordering is
required because the native runtime snapshots process environment during
startup; mutating the parent environment after creating a runtime is not a
valid replay configuration. The parent enforces bounded startup, consumer,
shutdown, and process-exit timeouts and always requests replay-server shutdown.

B8 is `passed locally`: direct actual-ABI and trimmed exact-package modes now
both prove fail-closed union selection and exact boundary-independent semantic
content. The committed-source external reproduction remains an
implementation-document entry requirement; these local runs do not substitute
for that provenance or for final product stream implementation and parity.

## Design impact

- Q17's bounded mechanism is the existing pull ABI: one ordinary native
  `next` call per demand. No stream-specific callback, acknowledgment field,
  channel, or unbounded queue is required by the design.
- Q18's restoration contract is feasible through the existing API table, but
  the nominal class / `_data` / typed-handle envelope is now normative for
  actual in-process CFFI media.
- This evidence closes the preimplementation semantic uncertainty in B8 and
  B9. It does not claim that the final public `BamlStream` or media classes are
  implemented, and it does not substitute for the committed-source external
  package/trim run.
