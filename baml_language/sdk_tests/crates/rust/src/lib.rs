//! Rust sdk-test crate. Three toolchain checks per fixture (rustfmt, clippy,
//! cargo test), declared below and expanded by
//! `sdk_test_harness_runner::rust::test_suite!`.
//!
//! Which ported test files each fixture actually compiles is a separate gate:
//! the `TEST_MODS` table in `sdk_tests/codegen/src/rust.rs` decides what
//! `generated/tests/main.rs` declares.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::rust::test_suite! {
    fixture docstrings_etc;
    fixture function_calls;
    fixture llm_functions;
    fixture type_shapes;
    fixture unsupported_only;
}
