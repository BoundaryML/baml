# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated `baml_sdk` crate
pins the matching version. See the BAML documentation for getting started:
<https://docs.boundaryml.com>.

Async calls are cancellation-safe: dropping a generated `_async` future (for example, when `tokio::time::timeout` expires) cancels the corresponding engine call instead of leaving it running detached. Completed result envelopes are limited to 32 MiB at the bridge boundary.
