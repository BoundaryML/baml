# C# actual-ABI and lifetime evidence

Status: B1 passed locally on 2026-07-17. The current Linux x64 actual-table
lifetime slice, clean package-default resolution, and fail-closed
override/diagnostic matrix pass, including a corrected regression that rejects
an unexpectedly successful load instead of catching its own sentinel. Final
product registry/SafeHandle races and the cross-RID runner matrix remain.

## Target artifacts

- target commit: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0` plus the
  recorded current-run ABI corrections
- native product version: `0.15.0`
- native library: fresh isolated current-source
  `release-bridge-cffi` Linux x64 artifact, `24,661,376` bytes
- native SHA-256:
  `dc8d399dbfaa14327be3eed25a52fa8661333ce211ca2d5a38fe0a833a323432`
- bytecode fixture:
  `sdk_tests/fixtures/function_calls/baml_src/main.baml`
- emitted bytecode: 683,918 bytes
- bytecode SHA-256:
  `44ec354587d912e222d0263e3bc8a944514195da2c134e9e1db6ce4e202d66f2`

The bytecode is emitted by the explicit ignored test
`csharp_abi_probe_tests::emit_function_calls_bytecode` in
`sdk_tests/harness_setup/src/lib.rs`. It uses the same current compiler and
Borsh serialization path as the SDK harness rather than a hand-authored or
historical payload.

The C# source is
`baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiLifetimeProbe`.
Project SHA-256:
`bba600afe92c162eaa23a73272ddd42c826cd7dbdca819c5a60e513c77303636`.
Probe source SHA-256:
`423930168205f4cb869f5161ea07fb178596326fca78a3a6cb4721de019a23e9`.
Shared explicit native-probe mode target SHA-256:
`01445658f4cce7e9531f6c1154e8ef1924974660b1f8f86da0b035da680c0776`.
The target freezes the evidence identity to
`Baml.Bridge.MultiRidPackageProbe` version `0.0.0-b4`; explicit package-ID or
version overrides fail before restore, while fresh exact-package and direct
restore/build sequences both pass with zero warnings.

The current source also accepts the exact sentinel `package-default` for the
manual eight-RID verifier. That mode installs no resolver and therefore uses
ordinary .NET package probing. A local evidence-only multi-RID package
mechanics run executed the same current bytecode and all outputs above through
that mode; duplicated local native bytes are not cross-RID evidence.

## Commands

```bash
cd baml_language
env RUSTC_WRAPPER= \
  BAML_CSHARP_ABI_PROBE_BYTECODE=/tmp/baml-csharp-b1-function-calls.bytecode \
  cargo test -p sdk_test_harness_setup \
  csharp_abi_probe_tests::emit_function_calls_bytecode \
  -- --ignored --exact --nocapture

env RUSTC_WRAPPER= \
  CARGO_TARGET_DIR=/root/baml-current-native-evidence.NGfRFQ/cargo-shipping \
  cargo build -p bridge_cffi \
  --profile release-bridge-cffi \
  --target x86_64-unknown-linux-gnu

cd ..
dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiLifetimeProbe/Baml.Bridge.AbiLifetimeProbe.csproj \
  --configuration Release \
  --nologo \
  -p:BamlNativeProbeMode=Direct

dotnet run \
  --project baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiLifetimeProbe/Baml.Bridge.AbiLifetimeProbe.csproj \
  --configuration Release \
  --no-build \
  --no-restore \
  -p:BamlNativeProbeMode=Direct -- \
  /root/baml-current-native-evidence.NGfRFQ/cargo-shipping/x86_64-unknown-linux-gnu/release-bridge-cffi/libbridge_cffi.so \
  0.15.0 \
  /tmp/baml-csharp-b1-function-calls.bytecode
```

The C# Release build completed with zero warnings and zero errors. Exact run
output:

```text
api_v1_size=176
product_version=0.15.0
bytecode_bytes=683918
hello_result_wire=0a0d1a0b68656c6c6f20776f726c64
registration_version_conflict=fail_closed
bytecode_invalid_then_valid=ok
ordinary_calls=2/2
utf8_binary_boundary=ok
structured_error_and_decode_failure=ok
pre_and_inflight_cancellation=ok
media_handle_clone_release=ok
owned_buffers_released=15
callback_boundary=contained
```

## Covered behavior

- the sole source-generated `baml_get_api_v1` import and all required typed
  table fields;
- exact ABI/table/product version and bridge-registration checks, including a
  wrong version, identical idempotent registration, and conflicting language;
- empty/invalid bytecode diagnostics followed by valid current-compiler
  initialization;
- borrowed call inputs and copied callback outputs;
- a nullary string-literal call and a string round trip containing non-ASCII,
  embedded NUL, and control bytes;
- structured unknown-function failure and a malformed-result decode failure;
- zero cancellation rejection, nonzero pre-registration cancellation,
  completed-ID reservation, and bounded in-flight cancellation of a dormant
  ten-second sleep fixture;
- image URL/base64 construction and eager access, MIME/null representation,
  invalid UTF-8, unsupported kind, null outputs, handle-type mismatch,
  clone independence, original/clone release, and invalid released handles;
- exact release of every exercised native-owned buffer, including zero-length
  success/optional buffers;
- static `[UnmanagedCallersOnly]` callbacks that copy borrowed memory,
  schedule continuations asynchronously, contain every exception, and ignore
  a synthetic duplicate/late completion.

## Clean packaged consumer and loader diagnostics

The repository-only
`Baml.Bridge.NativeAssetProbe.0.0.0-b1.nupkg` contains the exact native
library only at
`runtimes/linux-x64/native/libbridge_cffi.so`; the raw package SHA-256 is
`6e758353c6cb249a4da968f25486d42cb284780062250ea9509d446f5f9f22f1`.
The isolated consumer cache contained only
`baml.bridge.nativeassetprobe`.

The clean
`baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.PackagedAbiConsumer`
was restored from that feed and published outside the repository. The audit
image lacks the portable `linux-x64` host/runtime packs, so the executable
local publish used the installed `ubuntu.26.04-x64` pack. .NET correctly
selected the package's sole canonical `linux-x64` native asset through RID
fallback. The output contained exactly one native library at its root, with
SHA-256
`cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`,
byte-identical to the packed input.

Execution from `/tmp`, with no override and no repository working-directory
relationship, reported:

```text
product_version=0.15.0
resolution=package-default
packaged_getter_table=ok
```

Question 1's one durable maintainer/source-build mechanism is now frozen as
`BAML_BRIDGE_CSHARP_NATIVE_LIBRARY`. The assembly-owned resolver snapshots it
once, lazily, at first native resolution. Unset delegates to normal .NET
probing. Set requires an existing absolute file and loads exactly that file;
there is no packaged fallback. A valid override reported
`resolution=absolute-override`.

The published consumer then passed fresh-process negative cases for:

- relative override;
- missing absolute file;
- non-library file/wrong format;
- loadable library without `baml_get_api_v1`;
- getter returning null;
- ABI version 999;
- 168-byte truncated table versus the 176-byte required prefix;
- null required `register_bridge` field;
- product version `9.9.9` versus expected `0.15.0`.

The original `failure` driver used a broad exception filter that could catch
its own “expected loading to fail, but loaded” sentinel. The corrected driver
catches only `NativeProbeLoadException` in its native-failure branch. A clean
restore into a new cache compared the restored `.nupkg` byte-for-byte with
`Baml.Bridge.NativeAssetProbe.0.0.0-b1.nupkg`, then published and repeated all
eight invalid-override/table cases in separate processes. A dedicated
regression invoked `failure` with a valid override: the process exited
nonzero with
`expected native loading to fail with expected-marker, but loaded 0.15.0`.
This proves that a packaged or override fallback can no longer satisfy the
negative harness.

The durable C fixtures are
`tests/native_fixtures/missing_getter.c` and
`tests/native_fixtures/table_diagnostics.c`; the latter builds with C11,
`-Wall -Wextra -Werror`, no undefined symbols, and the canonical generated
header. Every explicit invalid override failed while the valid package asset
was present, proving fail-closed behavior instead of fallback.

Source SHA-256 values:

- native-asset package project:
  `f85ab11bb04c2a18e534232de9ad39088d504b02f86240cb5c1baa573553abf3`
- packaged-consumer project:
  `6e704d8de19efff8f08c4bf753a15d11fd443bc4f94649f4bffc801c550d0d39`
- packaged-consumer source:
  `274abadf03d4aecb494692ad47a2d0f398beaade80259d8535b1c8cb84b540f9`
- missing-getter fixture:
  `fce87d73435d9e3ae63c01bd9d0f310f7498eb718d945416561334fba2a8a484`
- table-diagnostic fixture:
  `b5b118f94949385f411b5a8bb5f6d2987445cb967dc7187de262c670f6dea281`

## ABI findings fixed by the gate

The public header previously said unknown/completed cancellation IDs return
failure, while the engine deliberately reserves any nonzero not-yet-active ID
as pre-cancelled so cancellation cannot lose a dispatch-registration race.
`bridge_cffi` Rust docs and the generated header now state the actual stable
contract: zero or no runtime returns 1; an active runtime accepts and reserves
every nonzero ID and returns 0.

The process-wide call-ID source previously used wrapping `fetch_add`, which
could eventually emit zero and reuse identifiers. `CallId::try_next()` now
exhausts permanently after allocating Current Canary's configured monotonic
range `1_000_000..=u64::MAX`; IDs below one million remain deliberately
reserved for internal/test use. `TASK/design.md` explicitly amends its earlier
“complete nonzero domain” wording to this compiled contract.
The C ABI returns zero as the exhaustion sentinel and managed code must fail
without dispatch. `cargo test -p sys_types call_id_ --lib -q` passes two tests
covering the exact `MAX-1`, `MAX`, zero-exhausted sequence and 16,384
concurrent unique nonzero allocations. The generated-header drift suite also
passes (3 passed, 1 intentionally ignored).

## Remaining B1 closure

- Run the packaged asset on every claimed RID runner; Linux x64 is the only
  actual native host here.
- Move the probe's lifecycle logic into the final runtime registry and execute
  deterministic product tests for cancellation/result/error races,
  setup-failure cleanup, callback-ID exhaustion, and naturally arriving
  duplicate/late callbacks.
- Cover `SafeHandle` leases under concurrent call/dispose in the final managed
  implementation; this slice proves native clone/release semantics directly.
