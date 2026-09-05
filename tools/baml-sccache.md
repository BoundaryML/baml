# Shared developer sccache

The repository's Cargo builds use `tools/baml-sccache` on POSIX and the native `tools_sccache` crate on Windows and macOS. See [`baml_language/tools_sccache/README.md`](../baml_language/tools_sccache/README.md) for credential precedence, Infisical human-session setup, failure behavior, security properties, opt-out controls, and the manual smoke test.
