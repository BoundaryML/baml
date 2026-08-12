# Primitive exact-package consumer

`verify.sh` regenerates the authoritative primitive client, or accepts an
already-generated source directory for cross-RID runners. It copies only the
two generated C# files plus this existing-project consumer into an isolated
directory, restores the exact local `baml-bridge` package into a fresh cache,
publishes for the requested RID, and executes without the native-library
override. It also proves unsupported-RID and NativeAOT diagnostics,
exact-version restore failure, exact native-byte selection, and rejects
source, tooling, protocol, loose-bytecode, multi-native, or repository-path
leakage.
