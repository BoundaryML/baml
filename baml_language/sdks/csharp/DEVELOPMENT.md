# C# bridge development

Run Rust commands from `baml_language`:

```sh
RUSTC_WRAPPER= cargo test -p sdkgen_csharp --lib
RUSTC_WRAPPER= cargo test -p bex_engine --test opaque_resource_handles
RUSTC_WRAPPER= cargo test -p sdk_test_csharp generated_baml_clients_are_not_tracked --lib
```

The full C# fixture suite is prepared by `sdk_test_harness_setup`; it invokes
the public C# generation facade twice and verifies deterministic manifests
before running the .NET consumers. Generated fixture clients are disposable
test output and must remain untracked.

For a canonical-schema change, build `bridge_ctypes` to refresh its Rust and
Python consumers, then regenerate and verify the checked-in C++ client:

```sh
RUSTC_WRAPPER= cargo build -p bridge_ctypes
RUSTC_WRAPPER= cargo test -p sdkgen_cpp --test pb_generation regenerate -- --ignored --exact
RUSTC_WRAPPER= cargo test -p sdkgen_cpp --test pb_generation
```

The C# project generates its protobuf classes during `dotnet build`. The
`Baml.Bridge.ProtocolProbe` project is the direct C# schema consumer. The
`emit_optional_host_call_vectors` binary belongs to the C# SDK test crate and
supplies Rust-produced callback vectors to that probe.

`bridge_csharp/src/Values/BamlUnion.cs` is a checked-in mechanical runtime
source. Regenerate it after changing the union surface, then verify that the
checked-in bytes match the C#-owned generator:

```sh
dotnet run --project sdks/csharp/bridge_csharp/tools/Baml.UnionGenerator/Baml.UnionGenerator.csproj --configuration Release -- --write sdks/csharp/bridge_csharp/src/Values/BamlUnion.cs
dotnet run --project sdks/csharp/bridge_csharp/tools/Baml.UnionGenerator/Baml.UnionGenerator.csproj --configuration Release -- --check sdks/csharp/bridge_csharp/src/Values/BamlUnion.cs
```

The generator emits arities 2 through 32 deterministically with UTF-8 and LF
line endings. The C# SDK test crate runs the same drift check in CI.

Release platform metadata declares `artifacts.cffi` and `artifacts.csharp` as
sibling records. C# entries name the RID-local native asset and require the
generic CFFI artifact for the same target; a generic CFFI target does not imply
a C# package. `build2-bridge-cffi.reusable.yaml` produces only generic native
artifacts. `verify-csharp-product-slice.reusable.yaml` assembles and exercises
the eight-RID NuGet package, and `publish2-csharp-sdk.yaml` publishes that
verified package.
