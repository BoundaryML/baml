//! Rust sdk-test crate. The `#[test]` suite — three toolchain tests
//! per fixture (rustfmt, clippy, cargo test) plus the shared
//! `build_diagnostics::no_build_failures` — is generated below by
//! `sdk_test_harness_runner::rust::test_suite!()`. The macro
//! `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::rust::run_all` (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::rust::test_suite!();
