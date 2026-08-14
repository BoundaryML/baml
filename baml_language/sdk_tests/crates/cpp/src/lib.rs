//! C++ sdk-test crate. The `#[test]` suite — two toolchain tests per
//! fixture (compile, run) plus the shared
//! `build_diagnostics::no_build_failures` — is generated below by
//! `sdk_test_harness_runner::cpp::test_suite!()`. The macro `include!`s the
//! OUT_DIR scaffold emitted by `sdk_test_harness_setup::cpp::run_all`
//! (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::cpp::test_suite!();
