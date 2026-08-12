//! Node TypeScript sdk-test crate. The `#[test]` suite covers Node
//! `tsc`/Vitest runners plus the shared
//! `build_diagnostics::no_build_failures` — is generated below by
//! `sdk_test_harness_runner::typescript::test_suite!()`. The
//! macro `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::typescript::run_all` (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::typescript::test_suite!();
