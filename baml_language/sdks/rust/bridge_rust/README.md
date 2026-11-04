# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated `baml_sdk` crate
pins the matching version. See the BAML documentation for getting started:
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

If the class has a method that is also named `new`, the method keeps that
name and the constructor becomes `new_`.

## Logging

This crate prints nothing on its own. It logs through the
[`log`](https://docs.rs/log) facade with targets under `baml_bridge`, so the
messages show up in whatever logger your app installs (`env_logger`, or a
`tracing` subscriber with `tracing-log`, which `tracing-subscriber` enables by
default). Try `RUST_LOG=baml_bridge=debug` to see how the BAML library is
found and loaded.
