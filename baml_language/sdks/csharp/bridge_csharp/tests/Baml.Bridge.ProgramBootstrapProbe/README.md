# Generated program/bootstrap evidence probe

This repository-only .NET 10 fixture compiles a canonical multi-file BAML
program as one private hexadecimal generated array. It verifies compiler-byte
identity, SHA-256 metadata, `Lazy` concurrency/failure caching, process-global
same-program reuse and conflict rejection, pre-native integrity failure, and
actual native rejection of structurally corrupt matching-fingerprint bytes.

Its `boundary` mode loads the compiled private field, compares every byte and
its SHA-256 with a deterministic synthetic input, and scans the generated
source for exactly one private `byte[]` and no Base64/resource alternate.
Synthetic bytes and generated source are normal ephemeral tool outputs; they
are not checked into the repository.

The generated carrier is supplied explicitly to this evidence project through
`BamlGeneratedProgramSource`. This is not a product MSBuild generation target;
the production workflow remains explicit `baml generate` followed by an
ordinary user-project build. Builds must also select
`BamlNativeProbeMode=Direct` or `Package`; package mode requires the isolated
evidence feed.
