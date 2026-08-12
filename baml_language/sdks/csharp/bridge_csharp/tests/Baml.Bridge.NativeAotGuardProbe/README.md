# NativeAOT rejection evidence probe

This repository-only project exercises the exact `BAML0019` build diagnostic
that the final package must ship through `buildTransitive`. Normal JIT builds
remain valid. Setting `PublishAot=true` stops before compilation and produces
no application artifact.

There is deliberately no opt-out property: NativeAOT is unsupported in v1,
while normal, trimmed, single-file, and trimmed single-file JIT remain the
supported deployment modes. The fixture requires explicit
`BamlNativeProbeMode=Direct` for its checked-in target or `Package` with the
isolated exact-package feed.
