# BAML Ruby V1 bridge

This directory contains private runtime plumbing for generated Ruby/Sorbet SDKs. Generated code calls:

```ruby
Baml::Bridge.initialize!(compiled_program_bytes)
```

The bridge loads the absolute library path in `BAML_RUNTIME_PATH`, validates the complete V1 C table, requires an exact canonical BAML toolchain version, registers `Baml::Bridge` as bridge language `10` with its stamped bridge runtime version, and registers generated programs by canonical uint64 identity. It returns the key; identical imports reuse registration, and distinct programs coexist in the same library. Passing `runtime_key:` explicitly permits collision testing and rejects conflicting contents.

This checkpoint deliberately has no public gem packaging, runtime discovery, download, unload, reset, or function-call API. Packaging and the public gem name remain release work.
