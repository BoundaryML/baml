# BAML Ruby/Sorbet runtime

The `baml-bridge` gem runs generated BAML Ruby/Sorbet SDKs. It contains pure Ruby code and the committed protobuf clients, and depends on `ffi`, `google-protobuf` and `sorbet-runtime`. Ruby 3.2 or newer is required.

Add `gem "baml-bridge"` to your application's Gemfile. Add a Ruby generator to your BAML project and generate the SDK:

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

The gem ships `rbi/baml.rbi`, the runtime contract used to check generated code.
Tapioca includes it when it generates the gem's RBI. Without Tapioca, copy it to
your application's `sorbet/rbi/` directory, or add its installed path to
`sorbet/config`:

```sh
echo "$(bundle info --path baml-bridge)/rbi/baml.rbi" >> sorbet/config
```

Check it alongside your generated `baml_sdk.rb` and `baml_sdk/` sources.

Generated structs compare by their fields and bound generic type arguments.
Function types accept either a typed Proc or a `Baml::Closure` returned by BAML.

## Generic values and calls

Generated generic classes use Sorbet's normal `type_member` and `[]`. The engine
infers generic function arguments from values where possible. To bind them
explicitly, pass named tokens, for example `identity(5, _types: { T: Integer })`.
Tokens include `Integer`, `String`, `Float`, `T::Boolean`, `NilClass`, generated
classes and enums, and `Baml::Types.list(t)`, `.map(k, v)`, `.optional(t)` and
`.generic(Klass, *args)` for compound types. Sorbet's erased `Klass[T]` is not a
runtime token for a parameterized class; use `Baml::Types.generic(Klass, T)`.

For a host-built instance that must retain concrete runtime arguments, use:

```ruby
box = Baml.bind_types(BamlSdk::GenericTests::GenericBox[Integer].new(value: 5), Integer)
box.pair_with("hello", _types: { U: String })
```

`bind_types` returns the same instance and preserves its Sorbet type. Decoded
instances retain their runtime arguments automatically. `T::Struct#with` creates
a new instance and drops this binding, so bind the copy again when needed.
Explicit method bindings require a bound receiver; bare calls can infer from
an unbound receiver's fields.

## Native library lookup

The gem does not include or download the native bridge library. Install a bridge built for the gem's canonical BAML toolchain version in one of these locations:

1. Set `BAML_BRIDGE_PATH` to the absolute library filename. This overrides the cache. An invalid override is an error, with no fallback.
2. Otherwise, install the library at `$BAML_HOME/bridges/<version>/<target>/<filename>`. `BAML_HOME` defaults to `~/.baml`. An empty environment variable is treated as unset.

For example, version `0.22.2` on x86-64 glibc Linux uses:

```text
~/.baml/bridges/0.22.2/x86_64-unknown-linux-gnu/libbridge_cffi.so
```

Targets match the Rust bridge cache: `x86_64` or `aarch64`, followed by `apple-darwin`, `unknown-linux-gnu`, `unknown-linux-musl`, or `pc-windows-msvc`. The filename is `libbridge_cffi.dylib` on macOS, `libbridge_cffi.so` on Linux, or `bridge_cffi.dll` on Windows. The loader derives the target from Ruby's `RbConfig` and checks the bridge ABI and canonical toolchain version before use.

## Build locally

From this directory:

```sh
gem build baml-bridge.gemspec
gem install --local baml-bridge-*.gem
```

The gemspec reads `lib/baml/bridge/version.rb`, which `scripts/baml-language-version` stamps during releases. RubyGems normalizes prerelease versions for the package; the native cache still uses the canonical version, such as `0.22.3-nightly.20261018.a`.

The committed `google-protobuf` clients under `lib/baml_bridge/cffi/v1/` come from the authoritative CFFI V1 schemas. Regenerate them from `baml_language/` with:

```sh
cargo build -p bridge_ctypes
```

A clean regeneration must leave these clients unchanged. Publishing the gem and downloading the bridge are separate release work.
