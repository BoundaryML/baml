//! JSON-ish parser for BAML.
//!
//! This crate provides a JSON parser that handles common LLM output quirks:
//! - JSON wrapped in markdown code blocks
//! - Trailing commas
//! - Single-quoted strings
//! - Partial/incomplete JSON for streaming
//!
//! The main entry point is [`from_str`] which parses and coerces JSON to a target type.

mod coercer;
mod parser;
mod value;

pub use coercer::{Coercer, CoercionError};
pub use parser::{parse, ParseOptions, ParseError};
pub use value::{Value, Fixes};

use baml_types::{BamlValue, TypeIR};

/// Parse a JSON-ish string and coerce it to the target type.
///
/// This is the main entry point for parsing LLM responses.
pub fn from_str(
    target: &TypeIR,
    raw_string: &str,
    is_done: bool,
) -> Result<BamlValue, CoercionError> {
    // Handle simple string type - don't try to parse as JSON
    if target.is_string() {
        return Ok(BamlValue::String(raw_string.to_string()));
    }

    // Parse the raw string to a Value
    let parsed = parse(raw_string, ParseOptions::default(), is_done)?;

    // Coerce to the target type
    let coercer = Coercer::new(is_done);
    coercer.coerce(&parsed, target)
}

/// Parse a JSON-ish string and coerce it, allowing partial results.
///
/// This is used during streaming to get partial values.
pub fn from_str_partial(
    target: &TypeIR,
    raw_string: &str,
) -> Result<BamlValue, CoercionError> {
    from_str(target, raw_string, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_string_type() {
        let result = from_str(&TypeIR::string(), "hello world", true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::String("hello world".to_string()));
    }

    #[test]
    fn test_from_str_json_object() {
        let json = r#"{"name": "Alice", "age": 30}"#;
        let result = from_str(&TypeIR::class("Person"), json, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_str_markdown_wrapped() {
        let json = r#"```json
{"name": "Alice"}
```"#;
        let result = from_str(&TypeIR::class("Person"), json, true);
        assert!(result.is_ok());
    }
}
