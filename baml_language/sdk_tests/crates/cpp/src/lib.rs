//! C++ sdk-test crate. Two toolchain checks per fixture (compile, run),
//! declared below and expanded by `sdk_test_harness_runner::cpp::test_suite!`.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::cpp::test_suite! {
    fixture docstrings_etc;
    fixture function_calls;
    fixture llm_functions;
    fixture type_shapes;
    fixture unsupported_only;
}
