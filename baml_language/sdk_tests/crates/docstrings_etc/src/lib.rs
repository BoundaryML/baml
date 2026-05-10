//! Sdk-test crate. The harness — four `#[test]`s wrapping `uv sync`
//! and `uv run {ruff,pyright,pytest}` — is generated below by
//! `sdk_test_build::sdk_test_suite!`. See `sdk_tests/build/src/lib.rs`
//! for what the macro expands to and why `sync_only` exists.
#[cfg(test)]
sdk_test_build::sdk_test_suite!();
