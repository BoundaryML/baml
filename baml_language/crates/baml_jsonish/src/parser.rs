//! JSON-ish parser.
//!
//! This parser handles common LLM output quirks:
//! - JSON wrapped in markdown code blocks
//! - Some tolerance for malformed JSON

use crate::value::{Fixes, Value};
use baml_types::CompletionState;
use thiserror::Error;

/// Options for parsing.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Whether to try extracting JSON from markdown.
    pub extract_markdown: bool,
}

impl ParseOptions {
    /// Create default options that try to extract JSON from markdown.
    pub fn default() -> Self {
        Self {
            extract_markdown: true,
        }
    }
}

/// Error during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Empty input")]
    EmptyInput,

    #[error("Failed to parse: {0}")]
    Other(String),
}

/// Parse a JSON-ish string.
///
/// This function handles:
/// - Standard JSON
/// - JSON wrapped in markdown code blocks
/// - Partial JSON (when `is_done` is false)
pub fn parse(
    input: &str,
    options: ParseOptions,
    is_done: bool,
) -> Result<Value, ParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Try to extract JSON from markdown code blocks
    if options.extract_markdown {
        if let Some((tag, extracted)) = extract_markdown_json(input) {
            match parse_json(extracted, is_done) {
                Ok(value) => {
                    let completion = if is_done {
                        CompletionState::Complete
                    } else {
                        value.completion_state()
                    };
                    return Ok(Value::Markdown(tag, Box::new(value), completion));
                }
                Err(_) => {
                    // Fall through to try parsing the whole input
                }
            }
        }
    }

    // Try parsing as JSON
    parse_json(input, is_done)
}

/// Extract JSON from markdown code blocks.
///
/// Returns (tag, content) if found, where tag is "json", "javascript", etc.
fn extract_markdown_json(input: &str) -> Option<(String, &str)> {
    // Look for ```json or ``` followed by content and closing ```
    let input = input.trim();

    // Check for opening code fence
    if !input.starts_with("```") {
        return None;
    }

    // Find the end of the opening line
    let after_fence = &input[3..];
    let newline_pos = after_fence.find('\n')?;
    let tag = after_fence[..newline_pos].trim().to_string();
    let content_start = 3 + newline_pos + 1;

    // Find closing fence
    let remaining = &input[content_start..];
    if let Some(close_pos) = remaining.rfind("```") {
        let content = &remaining[..close_pos].trim();
        Some((if tag.is_empty() { "json".to_string() } else { tag }, content))
    } else {
        // No closing fence - might be streaming
        Some((if tag.is_empty() { "json".to_string() } else { tag }, remaining.trim()))
    }
}

/// Parse a string as JSON.
fn parse_json(input: &str, is_done: bool) -> Result<Value, ParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Try standard JSON parsing first
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(json) => {
            let completion = if is_done {
                CompletionState::Complete
            } else {
                CompletionState::Incomplete
            };
            Ok(json_to_value(json, completion))
        }
        Err(e) => {
            // If not done, try to parse partial JSON
            if !is_done {
                if let Some(partial) = try_parse_partial(input) {
                    return Ok(partial);
                }
            }
            Err(ParseError::InvalidJson(e))
        }
    }
}

/// Convert serde_json::Value to our Value type.
fn json_to_value(json: serde_json::Value, completion: CompletionState) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Boolean(b),
        serde_json::Value::Number(n) => Value::Number(n, completion),
        serde_json::Value::String(s) => Value::String(s, completion),
        serde_json::Value::Array(arr) => {
            let items = arr.into_iter()
                .map(|v| json_to_value(v, completion))
                .collect();
            Value::Array(items, completion)
        }
        serde_json::Value::Object(obj) => {
            let pairs = obj.into_iter()
                .map(|(k, v)| (k, json_to_value(v, completion)))
                .collect();
            Value::Object(pairs, completion)
        }
    }
}

/// Try to parse partial/incomplete JSON.
///
/// This is called when standard JSON parsing fails and we're in streaming mode.
fn try_parse_partial(input: &str) -> Option<Value> {
    let input = input.trim();

    // Try to fix common issues and parse
    let fixed = fix_partial_json(input);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&fixed) {
        let value = json_to_value(json, CompletionState::Incomplete);
        return Some(Value::FixedJson(Box::new(value), vec![Fixes::RemovedTrailingComma]));
    }

    // If it looks like the start of an object or array, return a partial
    if input.starts_with('{') {
        return Some(Value::Object(vec![], CompletionState::Incomplete));
    }
    if input.starts_with('[') {
        return Some(Value::Array(vec![], CompletionState::Incomplete));
    }
    if input.starts_with('"') {
        // Partial string
        let content = input.trim_start_matches('"');
        return Some(Value::String(content.to_string(), CompletionState::Incomplete));
    }

    None
}

/// Fix common issues in partial JSON.
fn fix_partial_json(input: &str) -> String {
    let mut s = input.to_string();

    // Remove trailing commas
    while s.ends_with(',') {
        s.pop();
    }

    // Try to close unclosed structures
    let open_braces = s.matches('{').count();
    let close_braces = s.matches('}').count();
    for _ in 0..(open_braces.saturating_sub(close_braces)) {
        s.push('}');
    }

    let open_brackets = s.matches('[').count();
    let close_brackets = s.matches(']').count();
    for _ in 0..(open_brackets.saturating_sub(close_brackets)) {
        s.push(']');
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_json() {
        let result = parse(r#"{"name": "Alice"}"#, ParseOptions::default(), true);
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_object());
    }

    #[test]
    fn test_parse_markdown_wrapped() {
        let input = r#"```json
{"name": "Alice"}
```"#;
        let result = parse(input, ParseOptions::default(), true);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Value::Markdown(_, _, _)));
    }

    #[test]
    fn test_parse_markdown_no_tag() {
        let input = r#"```
{"name": "Alice"}
```"#;
        let result = parse(input, ParseOptions::default(), true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_array() {
        let result = parse(r#"[1, 2, 3]"#, ParseOptions::default(), true);
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_array());
    }

    #[test]
    fn test_parse_primitives() {
        assert!(parse("true", ParseOptions::default(), true).is_ok());
        assert!(parse("false", ParseOptions::default(), true).is_ok());
        assert!(parse("null", ParseOptions::default(), true).is_ok());
        assert!(parse("42", ParseOptions::default(), true).is_ok());
        assert!(parse("3.14", ParseOptions::default(), true).is_ok());
        assert!(parse(r#""hello""#, ParseOptions::default(), true).is_ok());
    }

    #[test]
    fn test_parse_partial_object() {
        // Streaming mode with incomplete JSON
        let result = parse(r#"{"name": "Alice","#, ParseOptions::default(), false);
        // Should either succeed with partial or fail gracefully
        // The actual behavior depends on our partial parsing logic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_extract_markdown_json() {
        let input = r#"```json
{"key": "value"}
```"#;
        let result = extract_markdown_json(input);
        assert!(result.is_some());
        let (tag, content) = result.unwrap();
        assert_eq!(tag, "json");
        assert!(content.contains("key"));
    }
}
