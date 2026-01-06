//! Output parsing - convert LLM responses to BAML types.
//!
//! This module will integrate with baml_jsonish for JSON parsing
//! once that crate is copied over.

use ir_stub::TypeRef;

use crate::errors::ParseOutputError;
use crate::types::BamlValue;

/// Parse LLM output content to a BAML value.
///
/// Currently a stub - will integrate with baml_jsonish later.
pub fn parse_output(content: &str, _output_type: &TypeRef) -> Result<BamlValue, ParseOutputError> {
    // For now, try to parse as JSON, falling back to string
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(json) => Ok(json_to_baml_value(json)),
        Err(_) => {
            // If not valid JSON, return as string
            Ok(BamlValue::String(content.to_string()))
        }
    }
}

/// Parse LLM output content to a BAML value, allowing partial results.
///
/// Used during streaming to show intermediate results.
pub fn parse_output_partial(
    content: &str,
    _output_type: &TypeRef,
) -> Result<BamlValue, ParseOutputError> {
    // For partial parsing, be more lenient
    // Try JSON first, then fall back to string
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(json) => Ok(json_to_baml_value(json)),
        Err(_) => Ok(BamlValue::String(content.to_string())),
    }
}

/// Convert a serde_json::Value to a BamlValue.
fn json_to_baml_value(json: serde_json::Value) -> BamlValue {
    match json {
        serde_json::Value::Null => BamlValue::Null,
        serde_json::Value::Bool(b) => BamlValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                BamlValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                BamlValue::Float(f)
            } else {
                BamlValue::Float(0.0) // Fallback
            }
        }
        serde_json::Value::String(s) => BamlValue::String(s),
        serde_json::Value::Array(arr) => {
            BamlValue::List(arr.into_iter().map(json_to_baml_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_baml_value(v)))
                .collect();
            BamlValue::Map(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_object() {
        let content = r#"{"name": "Alice", "age": 30}"#;
        let result = parse_output(content, &TypeRef::new("object")).unwrap();

        if let BamlValue::Map(map) = result {
            assert_eq!(map.get("name").and_then(|v| v.as_str()), Some("Alice"));
            assert_eq!(map.get("age").and_then(|v| v.as_int()), Some(30));
        } else {
            panic!("Expected Map, got {:?}", result);
        }
    }

    #[test]
    fn test_parse_plain_text() {
        let content = "Hello, world!";
        let result = parse_output(content, &TypeRef::string()).unwrap();

        assert_eq!(result.as_str(), Some("Hello, world!"));
    }

    #[test]
    fn test_parse_json_array() {
        let content = r#"["a", "b", "c"]"#;
        let result = parse_output(content, &TypeRef::new("array")).unwrap();

        if let BamlValue::List(list) = result {
            assert_eq!(list.len(), 3);
        } else {
            panic!("Expected List, got {:?}", result);
        }
    }
}
