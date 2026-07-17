# C# union layout evidence

Status: current-target B2 evidence captured on 2026-07-17.

## Decision

The v1 `BamlUnion<T0, ..., TN>` family uses one typed field per arm plus an
explicit one-based active-case tag. Case zero remains the invalid default.

The alternative `object` payload plus tag is a constant 16-byte value on this
target and copies faster, but it allocates on every construction whose active
payload is a value type: 24 bytes for `long` and the enum, and 32 bytes for
`BigInteger`. The typed layout allocated zero bytes during construction and
matching for every tested closure. Because ordinary typed BAML calls must not
introduce per-value boxing allocations, v1 accepts the typed layout's larger
closed structs and copy cost. Arity 32 remains a hard limit and generated APIs
must avoid defensive copies.

This selects a private implementation layout, not a promise that callers may
depend on field offsets. Because the public values are structs, any later
layout change is nevertheless treated as a binary-versioning decision.

## Probe

Source:

- `baml_language/sdks/csharp/bridge_csharp/tools/Baml.Union.LayoutProbe/Program.cs`
- `baml_language/sdks/csharp/bridge_csharp/tools/Baml.Union.LayoutProbe/Baml.Union.LayoutProbe.csproj`

Source SHA-256:

- `Program.cs`: `e3511bb0117fba9de78d5f6110280f3dd174a5959c551631dd3a7cfecbabcb4a`
- project: `cc59b943cebe261b0c4979e21b124da8a42c10d8c9617307c57f9e97b2e358e2`

The standalone probe mechanically closes arities 2, 8, 16, and 32 over
reference, primitive, enum, `BigInteger`, generated-class-shaped, and mixed
arms. It measures `Unsafe.SizeOf<T>()`, array copy time, and
`GC.GetAllocatedBytesForCurrentThread()` around construction and matching. It
also asserts that two identical closed arm types remain distinct by case and
that the default value has no active case.

Environment:

- Ubuntu 26.04 x64 (`ubuntu.26.04-x64`)
- .NET SDK `10.0.110`, MSBuild `18.0.11`
- Microsoft.NETCore.App `10.0.10`
- C# `14.0`, Release configuration, warnings as errors

Commands:

```bash
dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tools/Baml.Union.LayoutProbe/Baml.Union.LayoutProbe.csproj \
  --configuration Release \
  --nologo

dotnet run \
  --project baml_language/sdks/csharp/bridge_csharp/tools/Baml.Union.LayoutProbe/Baml.Union.LayoutProbe.csproj \
  --configuration Release \
  --no-build
```

The build completed with zero warnings and zero errors. The run completed
successfully with the following exact output:

| Arity | Payload | Fields bytes | Payload/tag bytes | Fields copy ns/op | Payload/tag copy ns/op | Fields construct B/op | Payload/tag construct B/op | Fields match B/op | Payload/tag match B/op |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | reference | 24 | 16 | 10.66 | 5.75 | 0.00 | 0.00 | 0.00 | 0.00 |
| 2 | primitive | 24 | 16 | 5.36 | 1.43 | 0.00 | 24.00 | 0.00 | 0.00 |
| 2 | enum | 24 | 16 | 5.46 | 1.43 | 0.00 | 24.00 | 0.00 | 0.00 |
| 2 | bigint | 40 | 16 | 8.86 | 1.45 | 0.00 | 32.00 | 0.00 | 0.00 |
| 2 | class | 24 | 16 | 2.53 | 1.53 | 0.00 | 0.00 | 0.00 | 0.00 |
| 2 | mixed | 32 | 16 | 8.99 | 1.45 | 0.00 | 32.00 | 0.00 | 0.00 |
| 8 | reference | 72 | 16 | 11.24 | 1.51 | 0.00 | 0.00 | 0.00 | 0.00 |
| 8 | primitive | 72 | 16 | 5.83 | 1.51 | 0.00 | 24.00 | 0.00 | 0.00 |
| 8 | enum | 72 | 16 | 5.58 | 1.50 | 0.00 | 24.00 | 0.00 | 0.00 |
| 8 | bigint | 136 | 16 | 16.05 | 1.61 | 0.00 | 32.00 | 0.00 | 0.00 |
| 8 | class | 72 | 16 | 7.14 | 1.45 | 0.00 | 0.00 | 0.00 | 0.00 |
| 8 | mixed | 88 | 16 | 11.15 | 1.51 | 0.00 | 32.00 | 0.00 | 0.00 |
| 16 | reference | 136 | 16 | 16.70 | 1.72 | 0.00 | 0.00 | 0.00 | 0.00 |
| 16 | primitive | 136 | 16 | 11.82 | 1.41 | 0.00 | 24.00 | 0.00 | 0.00 |
| 16 | enum | 136 | 16 | 11.37 | 1.42 | 0.00 | 24.00 | 0.00 | 0.00 |
| 16 | bigint | 264 | 16 | 19.38 | 1.42 | 0.00 | 32.00 | 0.00 | 0.00 |
| 16 | class | 136 | 16 | 12.01 | 1.44 | 0.00 | 0.00 | 0.00 | 0.00 |
| 16 | mixed | 168 | 16 | 17.14 | 1.44 | 0.00 | 32.00 | 0.00 | 0.00 |
| 32 | reference | 264 | 16 | 21.20 | 1.43 | 0.00 | 0.00 | 0.00 | 0.00 |
| 32 | primitive | 264 | 16 | 15.53 | 1.41 | 0.00 | 24.00 | 0.00 | 0.00 |
| 32 | enum | 264 | 16 | 15.46 | 1.47 | 0.00 | 24.00 | 0.00 | 0.00 |
| 32 | bigint | 520 | 16 | 26.13 | 1.46 | 0.00 | 32.00 | 0.00 | 0.00 |
| 32 | class | 264 | 16 | 14.98 | 1.50 | 0.00 | 0.00 | 0.00 | 0.00 |
| 32 | mixed | 320 | 16 | 21.36 | 1.44 | 0.00 | 32.00 | 0.00 | 0.00 |

Timing values are target-specific observations, not performance guarantees.
The allocation deltas and semantic assertions are the decision-driving
results.
