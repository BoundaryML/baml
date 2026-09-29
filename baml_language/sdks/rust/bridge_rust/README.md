# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated `baml_sdk` crate
pins the matching version. See the BAML documentation for getting started:
<https://docs.boundaryml.com>.

## Provisioning the engine without a downloader

Set `default-features = false` on the `baml_bridge` dependency and provision the matching engine library through `BAML_LIBRARY_PATH` or `set_shared_library_path`. This omits the downloader, ureq, rustls, and their TLS backend dependencies. Local cache and system-path discovery remain available; a missing library returns an error.

Apply this dependency setting in the generated SDK's manifest too, retaining it as part of the downstream generation or patch process. Disabling defaults on an application's additional direct dependency does not override features enabled by the generated SDK.

The default `aws-crypto` feature enables downloading. `ring-crypto` and `external-crypto` also enable the `download` feature. `BAML_LIBRARY_DISABLE_DOWNLOAD=true` disables downloads at runtime but does not remove compiled dependencies.

The default `baml-defaults` feature enables arbitrary-precision JSON numbers. If using a local engine, keep `default-features = false` and add `features = ["baml-defaults"]` to retain that JSON behavior without enabling the downloader. Without `baml-defaults`, JSON conversion of large bigint values may fail or lose precision.
