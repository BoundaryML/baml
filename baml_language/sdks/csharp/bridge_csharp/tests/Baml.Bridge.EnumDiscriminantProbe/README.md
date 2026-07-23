# BAML C# enum discriminant conformance probe

This repository-only .NET 10 probe compiles the exact
`baml-csharp-enum-discriminant-v1` byte grammar documented in the C# SDK's
[`ABI.md`](../../../ABI.md).
It verifies four SHA-256/value golden vectors, typed segment boundaries,
member insertion/reordering stability, and fail-closed zero/collision
handling against the production generator implementation. It is not shipped
in `baml-bridge`.
