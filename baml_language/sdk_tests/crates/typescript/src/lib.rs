//! Node TypeScript sdk-test crate. Three checks per fixture (generated-ESM
//! shape, tsc, vitest) plus one bridge-wide `attw` check, declared below and
//! expanded by `sdk_test_harness_runner::typescript::test_suite!`.
//!
//! This crate owns the canonical TypeScript test corpus: `typescript_web`
//! copies each fixture's `customizable/` overlay from here into its own Web
//! and Workers trees.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::typescript::test_suite! {
    fixture docstrings_etc;
    fixture function_calls;
    fixture llm_functions;
    fixture type_shapes;
    fixture unsupported_only;
}
