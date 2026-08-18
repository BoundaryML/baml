# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated `baml_sdk` crate
pins the matching version. See the BAML documentation for getting started:
<https://docs.boundaryml.com>.

Generated functions with `image`, `audio`, `video`, or `pdf` parameters use `baml_bridge::media::{Image, Audio, Video, Pdf}`. Each type provides `from_url`, `from_file`, and `from_base64` constructors and can be passed directly to generated functions. A generic `media` parameter uses `baml_bridge::media::Media`, an enum over those concrete kinds plus `GenericMedia`.
