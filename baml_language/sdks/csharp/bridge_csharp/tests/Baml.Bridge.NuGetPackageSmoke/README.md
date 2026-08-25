# NuGet package smoke

These scripts test packaging with the stable `basic_calls` API. Language
feature coverage belongs in the normal C# SDK test suite.

`verify.sh` restores and executes the exact package for each RID. It also
checks package selection, unsupported RID and NativeAOT diagnostics, version
mismatch behavior, and package hygiene.

`verify-deployment.sh` checks trimmed and single-file deployment shapes.
