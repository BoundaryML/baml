# C# generated-program bootstrap and NativeAOT evidence

This record covers compiled design gates B12 and B13 against Current Canary.
It verifies the question-20 carrier with canonical compiler output and the
question-19 negative NativeAOT boundary. It does not substitute for B11's
representative final-product trim matrix.

## Target and sources

- Target baseline: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0` on
  `paulo/csharp-bridge`.
- Audit host: Linux x64; .NET SDK `10.0.110`; runtime `10.0.10`; C# 14 /
  `net10.0`.
- Native: fresh isolated current-source release build, 20,961,256 bytes,
  SHA-256
  `cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`;
  product version `0.15.0`.
- Carrier emitter:
  `baml_language/sdks/csharp/bridge_csharp/tools/Baml.BytecodeCarrierEmitter`.
- Bootstrap probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProgramBootstrapProbe`.
- NativeAOT guard probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.NativeAotGuardProbe`.

The carrier emitter is repository-only compiled evidence for the production
generator. It is explicitly invoked before an ordinary project build; it is
not an MSBuild target, runtime loader, or alternate bytecode format.

## Canonical bytecode and deterministic carrier

The repository-owned ignored emitter loaded the canonical multi-file
`function_calls` fixture through the current compiler:

```shell
cd baml_language
env \
  BAML_CSHARP_ABI_PROBE_BYTECODE=/tmp/baml-csharp-b13-function-calls-current.bytecode \
  RUSTC_WRAPPER= \
  cargo test -p sdk_test_harness_setup \
  csharp_abi_probe_tests::emit_function_calls_bytecode \
  --lib -- --ignored --exact --nocapture
```

Result: 1 passed. The complete six-file fixture produced 683,918 bytes with
SHA-256
`44ec354587d912e222d0263e3bc8a944514195da2c134e9e1db6ce4e202d66f2`.
It is byte-identical to the canonical artifact used by B1.

The warning-free Release emitter build and generation were:

```shell
dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tools/Baml.BytecodeCarrierEmitter/Baml.BytecodeCarrierEmitter.csproj \
  --configuration Release --nologo -p:NuGetAudit=false

dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tools/Baml.BytecodeCarrierEmitter/Baml.BytecodeCarrierEmitter.csproj \
  --configuration Release --no-build --no-restore -- \
  /tmp/baml-csharp-b1-function-calls.bytecode \
  /tmp/baml-csharp-b13-BamlProgram.g.cs 0.15.0
```

The generated source is 4,446,506 bytes with SHA-256
`3addbbff5c44c257467188b28ace9f06db77c5760107a707e9585f4f4a8b2937`.
A second isolated emission had the same digest and was byte-identical. Source
inspection and runtime parsing prove exactly one
`private static readonly byte[]`, lowercase two-digit hexadecimal literals,
one SHA-256 constant, one `Lazy<ProgramProbe>` with
`ExecutionAndPublication`, no Base64, and no resource path.

## B13: boundary-scale compiled lower bound

The emitter now has a checked-in `--synthesize` mode so this gate does not
depend on a large binary committed to the repository. It emits the
little-endian byte stream from the documented xorshift64* v1 algorithm, writes
that payload to an explicit absolute path, and sends it through the same exact
one-private-hexadecimal-`byte[]` source path as canonical compiler bytecode.
The large payload and generated source remain ephemeral under `/tmp`.

The strongest fixture that this audit host compiled and executed was 16 MiB:

```shell
dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tools/Baml.BytecodeCarrierEmitter/Baml.BytecodeCarrierEmitter.csproj \
  --configuration Release --no-build --no-restore -- \
  --synthesize 16777216 \
  /tmp/baml-b13-boundary-16m-v1/payload.bin \
  /tmp/baml-b13-boundary-16m-v1/BamlProgram.g.cs \
  0.15.0

/usr/bin/time -v dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProgramBootstrapProbe/Baml.Bridge.ProgramBootstrapProbe.csproj \
  --configuration Release --nologo --disable-build-servers \
  -p:NuGetAudit=false \
  -p:UseSharedCompilation=false \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlGeneratedProgramSource=/tmp/baml-b13-boundary-16m-v1/BamlProgram.g.cs \
  -p:BaseOutputPath=/tmp/baml-b13-boundary-16m-v1-bin/ \
  -p:BaseIntermediateOutputPath=/tmp/baml-b13-boundary-16m-v1-obj/

dotnet \
  /tmp/baml-b13-boundary-16m-v1-bin/Release/net10.0/Baml.Bridge.ProgramBootstrapProbe.dll \
  boundary \
  /tmp/baml-b13-boundary-16m-v1/payload.bin \
  /tmp/baml-b13-boundary-16m-v1/BamlProgram.g.cs
```

The net10.0/C# 14 Release compile completed with zero warnings and zero
errors. Shared compilation and build servers were disabled for a meaningful
measurement: elapsed compile time was 6 minutes 18.26 seconds and maximum
resident set size was 22,867,872 KiB. The 16,777,216-byte payload has SHA-256
`24b8a153f5fba087f5f422e3b3b89b2d7d4b92e906c6346611ca8af44876454e`.
Its 109,052,942-byte generated source has SHA-256
`61f1bef8d4605b3b92691e73c417aad1f39fb0f40a85f68559beb74bdac7cd97`;
the resulting 16,805,888-byte managed DLL has SHA-256
`51b7178887492c1b984c15c425611f71ac652f3daf1ab84a49f8473f53f363ae`.
A second emission produced byte-identical payload and source files.

The compiled probe loaded the generated private field, compared every byte,
recomputed the fingerprint, and streamed the 109 MB source inspection without
constructing a second source-scale parse:

```text
boundary_bytes=16777216
boundary_sha256=24b8a153f5fba087f5f422e3b3b89b2d7d4b92e906c6346611ca8af44876454e
boundary_source_bytes=109052942
boundary_compiled_carrier=executed
boundary_source=one_private_hex_array
boundary_alternate_carriers=absent
```

This is a tested lower bound, not a maximum or a release ceiling. The same
host successfully emitted 64 MiB and 32 MiB carriers, but isolated Roslyn
compilation was killed with exit 137 at 23,409,508 KiB and 23,182,584 KiB
peak RSS respectively. Those are host-capacity observations, not C# or product
limits. No 8 MiB or other arbitrary product threshold is introduced.
Production generation must attempt the canonical carrier and turn its own
generation failures into deterministic diagnostics. Product build validation
must likewise preserve an explicit compiler diagnostic when the supported
toolchain rejects generated source; an external host OOM kill is an
infrastructure-capacity failure, not evidence for a smaller product ceiling
or a reason to silently choose Base64, resources, chunked arrays, or loose
bytecode.

## B13: actual native bootstrap passes locally

The generated source was compiled into the existing probe project through an
explicit evidence-only input property:

```shell
env NUGET_PACKAGES=/tmp/baml-csharp-a3-trim-nuget \
  dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProgramBootstrapProbe/Baml.Bridge.ProgramBootstrapProbe.csproj \
  --configuration Release --nologo --no-restore \
  -p:NuGetAudit=false \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlGeneratedProgramSource=/tmp/baml-csharp-b13-BamlProgram.g.cs
```

Result: zero warnings/errors, including trim analysis. The actual native run
used:

```shell
env NUGET_PACKAGES=/tmp/baml-csharp-a3-trim-nuget \
  dotnet run --project \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProgramBootstrapProbe/Baml.Bridge.ProgramBootstrapProbe.csproj \
  --configuration Release --no-build --no-restore \
  -p:BamlNativeProbeMode=Direct \
  -p:BamlGeneratedProgramSource=/tmp/baml-csharp-b13-BamlProgram.g.cs -- \
  valid-native \
  /tmp/baml-csharp-b1-function-calls.bytecode \
  /tmp/baml-csharp-b13-BamlProgram.g.cs \
  /root/baml-current-native-evidence.NGfRFQ/libbridge_cffi.so
```

It reported:

```text
carrier_bytes=compiler_exact
carrier_sha256=verified
carrier_source=one_private_hex_array
bootstrap=lazy_concurrent_singleton
program_reuse=same_fingerprint
program_conflict=before_native
multi_file_surfaces=one_program
bytecode_bytes=683918
program_fingerprint=44ec354587d912e222d0263e3bc8a944514195da2c134e9e1db6ce4e202d66f2
native_initializations=1
```

The fixture starts 128 concurrent first callers split between two generated
namespace/function surfaces. All receive the same program object and the real
native initializer runs once. Re-registration with the same exact fingerprint
returns that object without native work. A different valid fingerprint throws
the structured conflict before native initialization.

Two fresh-process negative modes also passed:

```text
edited_byte_integrity=failed_before_native
corrupt_matching_fingerprint=native_rejected
initialization_failure=single_cached_instance
```

The first edits a canonical byte while retaining the generated fingerprint and
proves zero initializer calls. The second hashes structurally invalid bytes
correctly, reaches the actual native bytecode initializer once, and proves 32
concurrent `Lazy` callers observe the exact same cached structured exception.
There is no source fallback.

## Carrier deployment shapes

All executions below started from `/tmp`, with no source-tree working-directory
assumption:

| Shape | Output | Inventory/result |
| --- | --- | --- |
| Framework-dependent JIT | fresh current-package output | Managed DLL 712,192 bytes, apphost 78,256 bytes, PDB 347,184 bytes, and the exact 20,961,256-byte native sidecar; `valid-packaged` passed. |
| Self-contained single-file, native sidecar | fresh current-package output | One 74,299,869-byte executable, PDB 347,184 bytes, and the exact native sidecar; `valid-packaged` passed. |
| Self-contained single-file, native self-extraction | fresh current-package output | One 95,261,168-byte executable and PDB 347,184 bytes, with no sidecar; `valid-packaged` passed. The sole extracted native SHA-256 exactly matched the input. |

The executable single-file RID is the host's installed
`ubuntu.26.04-x64` runtime pack; the BAML native asset remains the canonical
Linux x64 library. Published inventories contain no `.baml`, `.proto`,
`.bytecode`, `.bamlc`, generator manifest, CLI, or loose program artifact.
The compiled managed carrier alone supplies the program bytes.

The final explicit-package-mode preflight also restored the normalized local
mechanics package through source mapping, byte-compared its cache copy, and
published both untrimmed `linux-x64` single-file forms. The sidecar inventory
was exactly the executable, its PDB, and one `libbridge_cffi.so`; the
self-extract inventory was exactly the executable and its PDB. Both executed
`valid-packaged` and reported:

```text
packaged_carrier=embedded_managed_source
packaged_native=default_resolution
packaged_bootstrap=ok
```

The local package still duplicates one Linux native across RID paths, so it is
not B4 or cross-RID evidence. Its `linux-x64` asset is nevertheless the exact
current native input, so the package is valid local B11 deployment evidence.
The workflow uses the same exact allowed-inventory check for all four
untrimmed/trimmed forms, with the PDB allowed when emitted.

The exact native sidecar SHA-256 in both applicable outputs, and the extracted
native in self-extraction mode, is
`cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`.

The final local B11 retry restored that normalized package through source
mapping and then passed the complete warning-free `linux-x64` representative
matrix:

- trimmed self-contained ABI/lifetime execution with one selected native;
- trimmed Protobuf/media and pull-stream execution, including 17 restored media
  handles, 79 exact media buffer releases, the permitted 19-or-20 initial
  chunk-boundary variation followed by strict extensions, and exact stream
  cancellation/release behavior;
- trimmed managed value/generic/dynamic execution;
- trimmed reflection-rooted success plus deliberate unrooted removal;
- trimmed eight-RID policy execution; and
- trimmed single-file native sidecar and self-extraction, both executing
  `valid-packaged` with exact allowed inventories (executable, optional PDB,
  and the native only for sidecar). The workflow additionally byte-compares
  each sidecar and each native captured under a dedicated bundle-extraction
  root to the exact package-cache asset.

B11 is therefore `passed locally`. The committed-source external workflow
must still reproduce this matrix against the frozen evidence package before
the implementation document is created.

Conclusion: the canonical byte-array, integrity, singleton, conflict, cached
failure, ordinary publish, and both single-file carrier contracts are
`passed locally`. Product generator/runtime integration and cross-RID
publication remain implementation/final-consumer work.

## B12: targeted NativeAOT rejection passes locally

The guard fixture imports one target with:

```xml
<Target
  Name="BamlRejectUnsupportedNativeAot"
  BeforeTargets="PrepareForBuild"
  Condition="'$(PublishAot)' == 'true'">
  <Error Code="BAML0019" Text="..." />
</Target>
```

An ordinary Release build completed with zero warnings/errors and the
executable reported:

```text
nativeaot_guard=normal_jit_allowed
```

The negative command was:

```shell
dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.NativeAotGuardProbe/Baml.Bridge.NativeAotGuardProbe.csproj \
  --configuration Release --nologo --no-restore \
  -p:NuGetAudit=false \
  -p:BamlNativeProbeMode=Direct \
  -p:PublishAot=true \
  -p:BaseOutputPath=/tmp/baml-csharp-b12-negative-bin/ \
  -p:BaseIntermediateOutputPath=/tmp/baml-csharp-b12-negative-obj/
```

It stopped with exactly:

```text
error BAML0019: baml-bridge does not support NativeAOT in v1. Use a
normal, trimmed, single-file, or trimmed single-file JIT publish instead.
```

Exit was nonzero before compilation; no application binary or assembly was
produced in the isolated output. A fresh current normalized evidence package
then repeated a normal JIT execution and the exact `BAML0019` failure through
its `buildTransitive` target. The guard project permanently excludes
`bin/**;obj/**`, so changing isolated output roots cannot recompile stale
generated attributes. The paired `BAML0010` check restores the package without
the deliberately unsupported RID and supplies `linux-s390x` only to the
no-restore build; this lets the package's bounded diagnostic fire before
NuGet attempts to resolve a nonexistent host pack. Both failures contained
only their assigned exact diagnostic and produced no application artifact.
There is no opt-out property. B12 is `passed locally`; final product package
integration remains implementation work.

## Source hashes

| Source | SHA-256 |
| --- | --- |
| carrier emitter project | `083bf25d9e1fcad0bff524b36ba3f2920a31ce2a79b85ce8707834c39db3a67e` |
| carrier emitter `Program.cs` | `f43563db4c92c386961effc5ef68d9f9a2ad911b0d80562b88c92ec4f22724f0` |
| bootstrap probe project | `3e3fd1645976bf9523f8fbfc5d9a4fd4bff49c25586c24be8c7ee327f4902ae5` |
| generated call surfaces | `99de4725721b34f6e3b5d191eb26cdb09a272245d52652a62107e593069d5d83` |
| native initializer | `ae702867a0edce38b6b86edc4ea3c7937acd6d72f2167d337936690e3aa75fb0` |
| bootstrap probe `Program.cs` | `502334fadf0e51b3e6e5047a85f9e30ed9387692d9a910047c7636c52f77f818` |
| probe runtime | `d1328805d690b1bd1d45a6dd4404f27cdac9e3b7f9b59b41c6aa84734acf7e77` |
| explicit native-probe mode target | `01445658f4cce7e9531f6c1154e8ef1924974660b1f8f86da0b035da680c0776` |
| NativeAOT probe project | `41a0602b8b6a3779b3c05956df8c158540f55a2b905b564270a7dab24056b720` |
| NativeAOT target | `385b28f1c2f9dce6c63bea683c01f37ba48b84882c212cfd10db9ed949093ce8` |
| NativeAOT `Program.cs` | `7ae6a7b4c5e56d7b767c0cf10827b8ce6b34e0694098de311135c02ad99cf7a1` |
