# C# protocol generation and package probe

Date: 2026-07-15

Scope: the local Linux x64 portion of design question 9 plus one native-bearing
Linux x64 package slice from question 10. This is not the complete question-10
eight-RID native packaging probe or managed build-host matrix.

## Frozen tool pair

- `Grpc.Tools`: `2.82.0`, build-only and `PrivateAssets="all"`
- bundled Linux x64 compiler: `libprotoc 35.0`
- `Google.Protobuf`: resolved `3.35.1`, published dependency range
  `[3.35.1,4.0.0)`
- target: `net10.0`, C# 14

The bridge consumes all four canonical schemas through a common `ProtoRoot`,
sets `GrpcServices="None"`, `Access="Internal"`, and
`OutputOptions="file_extension=.g.cs"`. A project-local MSBuild target changes
the generator's predicted outputs to the same `.g.cs` names before incremental
staleness and compile-item calculation.

## Commands and results

The sandbox denied `/dev/null` and `/dev/urandom`; the commands below were run
with a temporary `/tmp`-only preload shim providing null-device behavior and
serving .NET entropy reads from Linux `getrandom`. The shim is not product code.

```bash
cd baml_language/sdks/csharp/bridge_csharp
dotnet restore Baml.Bridge.slnx --force
dotnet build Baml.Bridge.slnx --no-restore -c Release
dotnet test --solution Baml.Bridge.slnx --no-restore -c Release
tools/pack-native.sh \
  /root/dev/baml/baml_language/target/release/libbridge_cffi.so \
  linux-x64 /tmp/codex-csharp-native-script-a
```

Results:

- clean build: 0 warnings, 0 errors
- runtime tests: 46 passed
- generated transport declarations: internal, namespace
  `BamlBridge.Cffi.V1`
- two isolated direct generations: byte-for-byte identical
- second unchanged MSBuild invocation: no generated `.g.cs` or `.protodep`
  timestamp changed
- imported-schema invalidation: touching `baml_handle.proto` regenerated
  Handle, Inbound, and Outbound, but not Type
- direct-schema invalidation: touching `baml_outbound.proto` regenerated only
  Outbound
- the repeatable wrapper produced byte-identical normalized packages on two
  independent runs

Generated-source SHA-256 values:

```text
3469310d8103bd55a7d7747f970ce1eb053316d2e2eb63b32681100afcfb3188  BamlHandle.g.cs
6e948f31a1a0f1def4b1035073db632d2a810b77dde5df0a9eff2d4edcb08fa4  BamlInbound.g.cs
11af1adb6d02b8b1137d2d66c1c580069a0b5eb632ee7a4b11caa42ae78a9988  BamlOutbound.g.cs
1692c078e763ce7a1ae3c8eec48469bd97326c39337ba74e960f334feca3952b  BamlType.g.cs
```

## Earlier managed-only checkpoint

The initial package checkpoint was managed-only and intentionally required the
development native-library override. It established that the package contains:

```text
lib/net10.0/Baml.Bridge.dll
README.md
baml-bridge.nuspec
NuGet metadata files
```

It did not contain `.proto` files, generated C# source, build/buildTransitive
targets, absolute checkout paths, or native assets. Its sole dependency was
`Google.Protobuf [3.35.1,4.0.0)`. `Grpc.Tools`, gRPC client/server packages, and
network transport packages did not flow into the dependency graph. The
managed-only size and digest were provisional and were superseded by the
native-bearing checkpoint below.

## Linux x64 native-bearing package

`Baml.Bridge.csproj` accepts an explicit native path and RID for this probe and
places the asset at the standard
`runtimes/linux-x64/native/libbridge_cffi.so` package path. The POSIX and
PowerShell wrappers validate the supported RID and canonical platform filename,
pack in an isolated temporary directory, normalize the unsigned package, and
move only the normalized artifact to the requested output directory.

Measured inputs and output:

| Artifact | Raw bytes | SHA-256 |
| --- | ---: | --- |
| stripped ELF x86-64 `libbridge_cffi.so` | 19,729,160 | `4fd82a5d676728c74424d19d92dbc90be97d44136a28f390c9b39cb98d822d31` |
| `Baml.Bridge.dll` | 723,968 | `d5dffd5030f25baee8336658e1a378ffe8fd2c70f6c0609c3a8c70d2bc273170` |
| normalized `.nupkg` | 7,579,481 | `6a25b5624af50a1899bfca727f97a98fde560ba9d296b100b3d5b3402a92c67e` |

The larger managed assembly includes the mechanically generated structural
union family for arities 2 through 32. The native library exports
`baml_get_api_v1`. Its dynamic dependencies are the Linux loader, `libgcc_s`,
`libm`, and `libc`; no build-tree dependency is present. The normalized package
has seven entries: README, NuGet metadata, the managed DLL, and the one native
RID asset. It contains no native asset at the package root and no native asset
for another RID.

Two direct `dotnet pack --no-build` operations were not byte-reproducible:

```text
93f2c912bdfb2de757d9951299e3e65b343fd8d47976fbbc2628855f36f9429e
dc8039a06959928297acf7662477b6da8b4c9d084b89d1ccc050f45f4cc3ef93
```

Inspection isolated the variation to NuGet-generated OPC metadata: a random
core-properties part filename and its relationship ID. The zero-dependency
`Baml.NuGet.Normalize` tool rejects signed packages and unsafe ZIP paths,
derives both identifiers from the core-properties content hash, sorts entries,
and uses a fixed ZIP timestamp. Normalizing either raw package, or running
`tools/pack-native.sh` independently, produced the identical output digest
above. `unzip -t` reported no errors and every ZIP entry timestamp is
1980-01-01 00:00.

## Clean package consumer

`/root/dev/baml-csharp-nuget-poc` restores `baml-bridge 0.15.0` from the exact
normalized package into a fresh `NUGET_PACKAGES` directory and compiles
generated program source with nullable warnings as errors. The run explicitly
unsets `BAML_BRIDGE_LIBRARY`, proving ordinary NuGet RID resolution.

The consumer `obj` tree contains only SDK-generated assembly/global-using
sources; it contains no transport source or protobuf generation inputs. Build
result: 0 warnings, 0 errors. Runtime output without a development override:

```text
sync=hello from the packed bridge
async=hello from the packed bridge
```

A fresh-cache repeat after generic, union, media, and handle support restored
and built with zero warnings, then ran sync/async calls with
`BAML_BRIDGE_LIBRARY` unset. A framework-dependent `linux-x64` publish contains
exactly the selected 19,729,160-byte `libbridge_cffi.so` and no other RID
directory or native variant.

## Remaining evidence

- macOS arm64 and Windows x64 clean generation/build hosts
- representative protocol vectors beyond the current primitive/error unit set
- trim and NativeAOT behavior
- native/managed version-skew tests
- all eight native RID inputs and one atomic multi-RID release assembler; the
  wrappers above deliberately package only the single-RID feasibility slice
- RID selection, unsupported-RID diagnostics, publish output, restore/cache
  size, pack time, registry size ceiling, and every required native runner
- frozen-plan provenance and non-compiling publisher workflow
