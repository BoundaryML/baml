# BAML Ruby/Sorbet runtime

The Ruby runtime runs generated BAML Ruby/Sorbet SDKs. It contains pure Ruby code and the committed protobuf clients, and depends on `ffi`, `google-protobuf` and `sorbet-runtime`. Ruby 3.2 or newer is required.

Add a Ruby generator to your BAML project and generate the SDK:

```sh
baml generate add ruby/sorbet
baml generate --output-dir ./generated
```

The generator uses `output_type = "ruby/sorbet"` and `naming_convention = "language"` in `baml.toml`. It embeds the program and runtime metadata in `generated/baml_sdk.rb` and `generated/baml_sdk/`. Put `generated/` on the application's load path:

```ruby
require "baml_sdk"

puts BamlSdk.hello_world
call = BamlSdk.hello_world_async
puts call.wait
```

Generated functions initialize the runtime on their first call. `BamlSdk.initialize!` initializes it eagerly. The process can load only one generated BAML program; a forked child must exec before using an inherited runtime.

## Sorbet types

`rbi/baml.rbi` is the runtime contract used to check generated code. Check
it alongside your generated `baml_sdk.rb` and `baml_sdk/` sources.

Generated structs compare by their fields.

## Native library lookup

Set `BAML_BRIDGE_PATH` to the absolute filename of a native bridge library built for the runtime's canonical BAML toolchain version. The loader checks the bridge ABI and canonical toolchain version before use.

## Build locally

The committed `google-protobuf` clients under `lib/baml_bridge/cffi/v1/` come from the authoritative CFFI V1 schemas. Regenerate them from `baml_language/` with:

```sh
cargo build -p bridge_ctypes
```

A clean regeneration must leave these clients unchanged.
