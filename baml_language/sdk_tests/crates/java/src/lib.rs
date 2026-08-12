//! Java sdk-test crate. The `#[test]` suite — two toolchain tests per
//! fixture (`javac` via Gradle's `compileTestJava`, `junit` via
//! `gradle test`) plus the shared
//! `build_diagnostics::no_build_failures` and `setup_guard::ran` — is
//! generated below by `sdk_test_harness_runner::java::test_suite!()`.
//! The macro `include!`s the OUT_DIR scaffold emitted by
//! `sdk_test_harness_setup::java::run_all` (`build.rs`).
//!
//! Every test is `#[ignore]`d while `sdkgen_java` is a stub; tests are
//! un-ignored capability by capability as the Java bridge lands (see
//! `sdks/agent-docs/bridge-ref/ref-java-state-of-completeness.md`).
#[cfg(test)]
sdk_test_harness_runner::java::test_suite!();
