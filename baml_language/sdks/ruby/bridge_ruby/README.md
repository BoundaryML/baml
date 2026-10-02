# BAML Ruby V1 bridge

This directory contains private runtime plumbing for generated Ruby/Sorbet SDKs. Generated code calls:

```ruby
Baml::Bridge.initialize!(compiled_program_bytes)
Baml::Bridge.call(compiled_program_bytes, "user.hello_world", {})
```

Generated functions initialize automatically on first use and wait synchronously for the native result. Calls support primitives, generated classes and enums, optionals, lists, and maps. The generator registers exact wire names and Ruby field names before calls; class results use the declared `T::Struct`, not a streaming partial. Typed throws raise `Baml::Error`; ordinary panics raise `Baml::PanicError`. Both expose `type_name` and `message`; decoding full error payloads and traces is deferred. Other value kinds and process-exit panics raise an explicit not-yet-supported error.

The bridge loads the absolute library path in `BAML_RUNTIME_PATH`, validates the complete V1 C table, requires an exact canonical BAML toolchain version, registers `Baml::Bridge` as bridge language `10` with its stamped bridge runtime version, and initializes one exact generated program per process.

The committed `google-protobuf` clients under `lib/baml_bridge/cffi/v1/` are generated from the authoritative CFFI V1 schemas. Regenerate them from `baml_language/` with:

```sh
cargo build -p bridge_ctypes
```

The build uses vendored protoc. A clean regeneration must leave the committed clients unchanged.

This checkpoint deliberately has no public gem packaging, runtime discovery, download, unload, reset, or project-switching API. Packaging and the public gem name remain release work.
