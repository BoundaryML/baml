# C# bridge architecture

The C# product has three C#-owned layers:

- `sdkgen_csharp` converts a compiler `CodegenModel` and bytecode into a
  complete generated source tree. Its public boundary is
  `generate_into(CSharpGenerateRequest) -> GenerationReport`.
- `bridge_csharp/src` is the managed runtime shipped as `baml-bridge`. It owns
  managed values, generated-code support, callbacks, streams, native loading,
  and wire codecs.
- `sdk_tests/crates/csharp` owns executable generator fixtures and the small
  Rust fixture emitter used by C# protocol conformance tests.

The CLI discovers a C# generator, resolves its conventional `baml_sdk` directory, and calls the generation facade. `output_dir` selects the parent of that target-owned directory, matching every other generator. Directory staging, manifests, collision checks, and atomic replacement stay inside `sdkgen_csharp`.

Shared compiler and protocol crates expose language-neutral information only.
In particular, the compiler preserves union discovery order; the C# generator
normalizes unions before allocating names or rendering any C# layer. Resource
classes are distinguished by compiler-emitted host-boundary representation
metadata, not a C# or runtime list of class names.

The checked-in files under `bridge_csharp/src/Generated/V1` are C#-owned
runtime support for source produced by `sdkgen_csharp`; they are maintained and
tested as ordinary source. They are not protobuf output and have no separate
regeneration command. C# protobuf classes are generated into `obj/` by
`Grpc.Tools` from the canonical `bridge_ctypes/types` schemas during build and
are never checked in.

Generated fixture `baml_sdk/` directories are recreated by the SDK test harness and ignored. `sdk_test_csharp::generated_baml_clients_are_not_tracked` guards that policy.

See [ABI.md](ABI.md) for the native and wire contracts and
[DEVELOPMENT.md](DEVELOPMENT.md) for generation, testing, and release commands.
