//! Swift sdk-test crate. The `#[test]` suite — one toolchain test per
//! fixture (`swift test`, which builds first) plus the shared
//! `build_diagnostics::no_build_failures` — is generated below by
//! `sdk_test_harness_runner::swift::test_suite!()`. The macro
//! `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::swift::run_all` (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::swift::test_suite!();
