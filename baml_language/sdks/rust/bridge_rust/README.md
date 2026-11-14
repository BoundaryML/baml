# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated SDK crate pins
the matching version. The generated crate is named `baml_sdk` unless the
generator sets `package_name`, which names both its Cargo package and the
library you `use`:

```toml
[generator.rust]
output_type = "rust"
naming_convention = "language"
package_name = "alien_api"   # written to ./alien_api; `use alien_api::...`
```
 See the BAML documentation for getting started:
<https://docs.boundaryml.com>.

## Building generated structs

Each BAML class becomes a Rust struct with public fields and a `new`
constructor. `new` takes the required fields in declaration order and sets
every optional field to `None`. Set optional fields with struct update
syntax:

```rust
let request = CreateSetupLinkRequest {
    max_deployments: Some(5),
    ..CreateSetupLinkRequest::new(project_id, name)
};
```

Prefer this to a struct literal that lists every field. A literal stops
compiling when the class gains a field, even an optional one. A `new` call
keeps compiling until a required field is added. The structs don't
implement `Default`, so a forgotten required field is a compile error and
never sends `""` or `0`.

If one of the class's methods already uses the name `new`, the method keeps
its name. The constructor gets one more trailing underscore until the name is
free: `new_`, or `new__` if a method also uses `new_`.

## Logging

This crate prints nothing on its own. It logs through the
[`log`](https://docs.rs/log) facade with targets under `baml_bridge`, so the
messages show up in whatever logger your app installs (`env_logger`, or a
`tracing` subscriber with `tracing-log`, which `tracing-subscriber` enables by
default). Try `RUST_LOG=baml_bridge=debug` to see how the BAML library is
found and loaded.
