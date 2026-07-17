# C# union storage-layout probe

Date: 2026-07-16

## Decision

`BamlUnion<T0, ..., TN>` uses one private typed field per arm plus a one-based
case tag for arities 2 through 32. This is the v1 public-struct binary layout.
A later layout change requires an intentional binary compatibility decision.

The alternative `object` payload plus byte tag is always 16 bytes and copies
faster, especially at high arities. It necessarily boxes active value-type
arms, however: construction allocates 24 bytes for `long` and enum values and
32 bytes for `BigInteger` in this probe. The selected typed-field layout has no
construction or matching allocation for any measured arm. Avoiding a
per-value heap allocation on ordinary primitive, enum, bigint, and mixed union
paths takes precedence over the compact inactive-field representation. The
measured 32-arm worst case is 520 bytes and 27.28 ns per array copy for 32
`BigInteger` fields; BAML's current 16-arm built-in case measured 264 bytes and
19.77 ns per copy.

## Maintained probe

The source is
`baml_language/sdks/csharp/bridge_csharp/tools/Baml.Union.LayoutProbe` and is
part of `Baml.Bridge.slnx`. Run it with:

```bash
cd baml_language/sdks/csharp/bridge_csharp
dotnet run --project tools/Baml.Union.LayoutProbe --configuration Release
```

The probe mechanically closes the shipped union family at arities 2, 8, 16,
and 32. It uses `Unsafe.SizeOf<T>()`, timed array assignments, and
`GC.GetAllocatedBytesForCurrentThread()`. It compares repeated reference,
primitive, enum, `BigInteger`, generated-class-shaped reference, and mixed
arms. It also fails unless duplicate closed arm types retain distinct cases
and access through `default(BamlUnion<...>)` throws.

The recorded run used .NET SDK 10.0.109/runtime 10.0.9 on Linux x64 with an
8-vCPU AMD EPYC-Milan virtual machine at repository commit
`408b2be28afbf9005e7b50d1f5bd4621036ab1c9`. These compact measurements are a
layout decision spike rather than a general-purpose BenchmarkDotNet suite;
allocation results and relative size are the primary decision evidence.

## Results

| Arity | Payload | Fields bytes | Payload/tag bytes | Fields copy ns/op | Payload/tag copy ns/op | Fields construct B/op | Payload/tag construct B/op | Fields match B/op | Payload/tag match B/op |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 | reference | 24 | 16 | 10.36 | 5.60 | 0.00 | 0.00 | 0.00 | 0.00 |
| 2 | primitive | 24 | 16 | 5.67 | 1.55 | 0.00 | 24.00 | 0.00 | 0.00 |
| 2 | enum | 24 | 16 | 5.63 | 1.68 | 0.00 | 24.00 | 0.00 | 0.00 |
| 2 | bigint | 40 | 16 | 10.94 | 1.43 | 0.00 | 32.00 | 0.00 | 0.00 |
| 2 | class | 24 | 16 | 2.47 | 1.47 | 0.00 | 0.00 | 0.00 | 0.00 |
| 2 | mixed | 32 | 16 | 8.65 | 1.44 | 0.00 | 32.00 | 0.00 | 0.00 |
| 8 | reference | 72 | 16 | 11.35 | 1.62 | 0.00 | 0.00 | 0.00 | 0.00 |
| 8 | primitive | 72 | 16 | 5.61 | 1.40 | 0.00 | 24.00 | 0.00 | 0.00 |
| 8 | enum | 72 | 16 | 5.53 | 1.48 | 0.00 | 24.00 | 0.00 | 0.00 |
| 8 | bigint | 136 | 16 | 15.95 | 1.48 | 0.00 | 32.00 | 0.00 | 0.00 |
| 8 | class | 72 | 16 | 7.07 | 1.39 | 0.00 | 0.00 | 0.00 | 0.00 |
| 8 | mixed | 88 | 16 | 11.24 | 1.41 | 0.00 | 32.00 | 0.00 | 0.00 |
| 16 | reference | 136 | 16 | 16.46 | 1.45 | 0.00 | 0.00 | 0.00 | 0.00 |
| 16 | primitive | 136 | 16 | 11.89 | 1.45 | 0.00 | 24.00 | 0.00 | 0.00 |
| 16 | enum | 136 | 16 | 11.55 | 1.58 | 0.00 | 24.00 | 0.00 | 0.00 |
| 16 | bigint | 264 | 16 | 19.77 | 1.39 | 0.00 | 32.00 | 0.00 | 0.00 |
| 16 | class | 136 | 16 | 12.72 | 1.41 | 0.00 | 0.00 | 0.00 | 0.00 |
| 16 | mixed | 168 | 16 | 16.93 | 1.46 | 0.00 | 32.00 | 0.00 | 0.00 |
| 32 | reference | 264 | 16 | 22.25 | 1.41 | 0.00 | 0.00 | 0.00 | 0.00 |
| 32 | primitive | 264 | 16 | 15.36 | 1.40 | 0.00 | 24.00 | 0.00 | 0.00 |
| 32 | enum | 264 | 16 | 16.06 | 1.47 | 0.00 | 24.00 | 0.00 | 0.00 |
| 32 | bigint | 520 | 16 | 27.28 | 1.50 | 0.00 | 32.00 | 0.00 | 0.00 |
| 32 | class | 264 | 16 | 15.18 | 2.04 | 0.00 | 0.00 | 0.00 | 0.00 |
| 32 | mixed | 320 | 16 | 21.99 | 1.48 | 0.00 | 32.00 | 0.00 | 0.00 |
