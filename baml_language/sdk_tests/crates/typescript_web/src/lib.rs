//! Browser and Cloudflare Workers sdk-test crate. Six checks per fixture,
//! declared below and expanded by
//! `sdk_test_harness_runner::typescript_web::test_suite!`.
//!
//! No checked-in tests of its own: the corpus is copied from the sibling
//! `crates/typescript` package, with bridge imports rewritten for the Web
//! bridge.
//!
//! The fixture rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::typescript_web::test_suite! {
    fixture docstrings_etc;
    fixture function_calls;
    fixture llm_functions;
    fixture type_shapes;
    fixture unsupported_only;
}
