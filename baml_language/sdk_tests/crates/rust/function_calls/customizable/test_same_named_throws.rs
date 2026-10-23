//! A throws contract holding two same-named errors.
//!
//! `ParseIntOrJson`'s inferred contract is
//! `baml.errors.ParseError | baml.json.ParseError`. Both arms used to derive
//! the variant `ParseError`, so the whole function was skipped. Clashing arms
//! are now qualified by their innermost namespace segment.
//!
//! Python/TS erase `throws`, so there is no cross-language counterpart.

use baml_bridge::Error;
use baml_sdk::throws_test::{ErrorsParseErrorOrJsonParseError, ParseIntOrJson};

fn classify(s: &str, as_int: bool) -> String {
    match ParseIntOrJson(s.to_string(), as_int) {
        Ok(n) => format!("ok {n}"),
        Err(Error::Thrown { value, .. }) => match *value {
            ErrorsParseErrorOrJsonParseError::ErrorsParseError(_) => "int parse error".to_string(),
            ErrorsParseErrorOrJsonParseError::JsonParseError(_) => "json parse error".to_string(),
        },
        Err(other) => format!("unexpected: {other}"),
    }
}

// SDK_PARITY_LINT(skip): validates Rust-specific throws variant naming
#[test]
fn test_same_named_throws_ok_passes_through() {
    assert_eq!(classify("42", true), "ok 42");
}

// SDK_PARITY_LINT(skip): validates Rust-specific throws variant naming
#[test]
fn test_same_named_throws_selects_the_stdlib_parse_error() {
    assert_eq!(classify("4x", true), "int parse error");
}

// SDK_PARITY_LINT(skip): validates Rust-specific throws variant naming
#[test]
fn test_same_named_throws_selects_the_json_parse_error() {
    assert_eq!(classify("{", false), "json parse error");
}
