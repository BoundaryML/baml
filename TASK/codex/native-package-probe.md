# C# atomic native-package probe

Date: 2026-07-16

## Contract under test

`baml-bridge` is one NuGet package containing `Baml.Bridge.dll` and exactly one
native library for each supported RID:

- `linux-x64`, `linux-arm64`, `linux-musl-x64`, `linux-musl-arm64`
- `osx-x64`, `osx-arm64`
- `win-x64`, `win-arm64`

`sdks/csharp/bridge_csharp/tools/pack-all-native.sh` validates the staged input,
packs once, normalizes `.nupkg` and `.snupkg`, and inspects the native entries.
The release builder verifies the original native sidecar digests before staging
them and records the final package digests and size.

The package size ceiling is 200,000,000 bytes: 80% of the approximately 250 MB
limit documented by [NuGet.org](https://learn.microsoft.com/en-us/nuget/nuget-org/publish-a-package#package-size-limits).

## Local structural probe

The sandbox has only a Linux x64 release binary. To test the assembler without
misrepresenting platform support, the probe copied that same byte sequence into
all eight correctly named staging slots. This proves package mechanics and
determinism only; it does not prove binary architecture or loadability for the
other seven slots.

Two independent runs produced:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `baml-bridge.0.15.0.nupkg` | 59,346,002 | `ceac31082de515fdf95dca743113ae86483814a587b1dbc7a6a967f66c789791` |
| `baml-bridge.0.15.0.snupkg` | 128,309 | `9437c8b22941e1976c6de190ed760085b904ca3e8662f452a9b3755869efd043` |

Both pairs compared byte-for-byte equal. The primary package contained only the
eight expected `runtimes/{rid}/native/` entries, one managed assembly, the
README, NuGet metadata, and `buildTransitive/baml-bridge.targets`.

## Consumer diagnostics

A clean consumer restored from an isolated local feed containing only the probe
package and `Google.Protobuf 3.35.1`.

- `RuntimeIdentifier=linux-x64`: validation passed.
- `RuntimeIdentifiers=linux-x64;win-arm64`: validation passed.
- `RuntimeIdentifier=freebsd-x64`: failed with the supported-RID list.
- `RuntimeIdentifiers=linux-x64;freebsd-x64`: failed with the supported-RID list.

## Remaining release evidence

The release workflow now assembles from the shared eight-target native build and
fails if any input is absent. Production readiness still requires a successful
workflow run with the real binaries, binary-format/architecture/dependency
inspection, native package consumption on every available target runner, and
the external NuGet organization ownership/trusted-publisher configuration.
