# BAML C# enum discriminant evidence probe

This repository-only .NET 10 probe compiles the exact
`baml-csharp-enum-discriminant-v1` byte grammar frozen in `TASK/design.md`.
It verifies four SHA-256/value golden vectors, typed segment boundaries,
member insertion/reordering stability, and fail-closed zero/collision
handling. It is design evidence for the production generator implementation;
it is not shipped in `baml-bridge`.
