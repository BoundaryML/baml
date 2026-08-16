# BAML C# eight-RID package probe

This repository-only package is compiled once and assembled with all eight
immutable `bridge_cffi` artifacts by the manual C# entry-gate workflow. It
exists only to prove package size, deterministic normalization, RID selection,
clean restore/publish, native loading, and the transitive NativeAOT rejection
before production bridge implementation begins. It is not the supported
`baml-bridge` product package and must never be published.

The project packages a generic `runtimes/**/*` tree and requires exactly eight
native assets. The workflow derives the supported RID list from
`baml_language/crates/baml_release/platforms.json`, substitutes it into
`Baml.Bridge.MultiRidPackageProbe.targets.in`, and supplies that generated
target through `BamlGeneratedTargetsPath`. This keeps native package paths and
the transitive unsupported-RID diagnostic under one platform authority rather
than duplicating an eight-RID list in the package project.
