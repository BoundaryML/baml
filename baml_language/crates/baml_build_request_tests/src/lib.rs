//! Test utilities for BAML request building pipeline.
//!
//! This crate provides snapshot tests for:
//! - `render_prompt`: Prompt rendering with Jinja templates
//! - `render_raw_curl`: Raw curl command generation
//! - `build_request`: HTTP request construction
//!
//! ## Naming Convention
//!
//! Test fixtures use PascalCase filenames that directly map to function and test names:
//! - File: `TestCaseName.baml`
//! - Function: `FnTestCaseName`
//! - Test: `TestTestCaseName`
//!
//! For example, `OutputEnum.baml` contains `FnOutputEnum` and `TestOutputEnum`.

/// Derive the function name from a PascalCase fixture filename.
///
/// For `OutputEnum.baml`, returns `FnOutputEnum`.
pub fn derive_function_name(fixture_name: &str) -> String {
    let base = fixture_name.trim_end_matches(".baml");
    format!("Fn{}", base)
}

/// Derive the test name from a PascalCase fixture filename.
///
/// For `OutputEnum.baml`, returns `TestOutputEnum`.
pub fn derive_test_name(fixture_name: &str) -> String {
    let base = fixture_name.trim_end_matches(".baml");
    format!("Test{}", base)
}
