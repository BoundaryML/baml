//! Python + pydantic2 sdk-test crate. The `#[test]` suite — three
//! toolchain tests per fixture (ruff, pyright, pytest) plus the
//! shared `build_diagnostics::no_build_failures` — is generated
//! below by `sdk_test_harness_runner::python_pydantic2::test_suite!()`.
//! The macro `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::python_pydantic2::run_all` (`build.rs`).
#[cfg(test)]
sdk_test_harness_runner::python_pydantic2::test_suite!();
