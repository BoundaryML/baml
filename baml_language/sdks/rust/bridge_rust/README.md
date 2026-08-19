# baml_bridge

The BAML runtime bridge for Rust. Generated BAML SDKs (`baml-cli generate`
with `output_type = "rust"`) depend on this crate: it boots the BAML engine,
converts values across the boundary via the `BamlValue` trait, and surfaces
BAML's typed `throws` contracts as `Result<T, baml_bridge::Error<E>>`.

You normally don't add this crate by hand — the generated `baml_sdk` crate
pins the matching version. See the BAML documentation for getting started:
<https://docs.boundaryml.com>.

## Committing the generated crate

Generated SDK directories contain a catch-all `.gitignore` by default because most projects regenerate them during the build. If you commit the generated Rust crate for review or byte-parity checks, set `gitignore = false` in its `baml.toml` generator section so a normal `git add` includes the crate:

```toml
[generator.rust]
output_type = "rust"
naming_convention = "preserve-case"
gitignore = false
```
