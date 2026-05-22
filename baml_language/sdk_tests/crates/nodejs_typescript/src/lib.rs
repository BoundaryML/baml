//! Node.js + TypeScript sdk-test crate. The `#[test]` suite — two
//! toolchain tests per fixture (`tsc`, `jest`) plus the shared
//! `build_diagnostics::no_build_failures` — is generated below by
//! `sdk_test_harness_runner::nodejs_typescript::test_suite!()`. The
//! macro `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::nodejs_typescript::run_all` (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::nodejs_typescript::test_suite!();
