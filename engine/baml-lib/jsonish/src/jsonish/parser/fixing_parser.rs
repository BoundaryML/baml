mod json_collection;
mod json_parse_state;

use anyhow::Result;
use baml_types::CompletionState;

use self::json_parse_state::JsonParseState;
use super::ParseOptions;
use crate::jsonish::{value::Fixes, Value};

pub fn parse(str: &str, _options: &ParseOptions) -> Result<Vec<(Value, Vec<Fixes>)>> {
    // Try to fix some common JSON issues
    // - Unquoted single word strings
    // - Single quoted strings
    // - Double quoted strings with badly escaped characters
    // - Numbers
    // - Numbers starting with a .
    // - Booleans
    // - Null
    // - Arrays
    // - Objects
    // - Comments
    // - Trailing commas
    // - Leading commas
    // - Unterminated comments
    // - Unterminated arrays
    // - Unterminated objects
    // - Unterminated strings

    let mut state = JsonParseState::new();

    let mut chars = str.char_indices().peekable();
    while let Some((count, c)) = chars.next() {
        let peekable = str[count + c.len_utf8()..].char_indices().peekable();
        match state.process_token(c, peekable) {
            Ok(increments) => {
                for _ in 0..increments {
                    chars.next();
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // If we still have a collection open, close it
    while !state.collection_stack.is_empty() {
        state.complete_collection(CompletionState::Incomplete);
    }

    // Determine what to return.

    match state.completed_values.len() {
        0 => Err(anyhow::anyhow!("No JSON objects found")),
        1 => state
            .completed_values
            .pop()
            .map(|(_name, value, fixes)| Ok(vec![(value, fixes)]))
            .unwrap_or(Err(anyhow::anyhow!("Failed to pop completed value"))),
        _ => {
            if state.completed_values.iter().all(|f| f.0 == "string") {
                // If all the values are strings, return them as an array of strings
                Ok(vec![(
                    Value::Array(
                        state
                            .completed_values
                            .into_iter()
                            .map(|f| {
                                let completion_state = f.1.completion_state().clone();
                                Value::FixedJson(f.1.into(), f.2)
                            })
                            .collect(),
                        CompletionState::Incomplete, // TODO: Is it complete?
                    ),
                    vec![Fixes::InferredArray],
                )])
            } else {
                // Filter for only objects and arrays
                let values: Vec<(Value, Vec<Fixes>)> = state
                    .completed_values
                    .into_iter()
                    .filter_map(|f| {
                        if f.0 == "Object" || f.0 == "Array" {
                            Some((f.1, f.2))
                        } else {
                            None
                        }
                    })
                    .collect();
                match values.len() {
                    0 => Err(anyhow::anyhow!("No JSON objects found")),
                    _ => Ok(values),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonish::{ParseOptions, Value};

    #[test]
    fn test_partial_array() {
        let opts = ParseOptions::default();
        let vals = parse("[12", &opts).unwrap();

        match vals[0].0.clone() {
            Value::Array(xs, array_cmplt) => {
                assert_eq!(xs.len(), 1);
                assert_eq!(array_cmplt, CompletionState::Incomplete);
                match &xs[0] {
                    Value::Number(n, n_cmplt) => {
                        assert_eq!(n, &serde_json::Number::from(12));
                        assert_eq!(n_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected number"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_partial_object() {
        let opts = ParseOptions::default();
        let vals = parse(r#"{"a": 11, "b": 22"#, &opts).unwrap();
        match &vals[0].0 {
            Value::Object(fields, obj_cmplt) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(obj_cmplt, &CompletionState::Incomplete);
                match (&fields[0], &fields[1]) {
                    ((key_a, Value::Number(a, a_cmplt)), (key_b, Value::Number(b, b_cmplt))) => {
                        assert_eq!(key_a.as_str(), "a");
                        assert_eq!(key_b.as_str(), "b");
                        assert_eq!(a, &serde_json::Number::from(11));
                        assert_eq!(b, &serde_json::Number::from(22));
                        assert_eq!(a_cmplt, &CompletionState::Complete);
                        assert_eq!(b_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected two numbers."),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_partial_object_newlines() {
        let opts = ParseOptions::default();
        let vals = parse("{\n \"a\": 11, \n \"b\": 22", &opts).unwrap();
        match &vals[0].0 {
            Value::Object(fields, obj_cmplt) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(obj_cmplt, &CompletionState::Incomplete);
                match (&fields[0], &fields[1]) {
                    ((key_a, Value::Number(a, a_cmplt)), (key_b, Value::Number(b, b_cmplt))) => {
                        assert_eq!(key_a.as_str(), "a");
                        assert_eq!(key_b.as_str(), "b");
                        assert_eq!(a, &serde_json::Number::from(11));
                        assert_eq!(b, &serde_json::Number::from(22));
                        assert_eq!(a_cmplt, &CompletionState::Complete);
                        assert_eq!(b_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected two numbers."),
                }
            }
            _ => panic!("Expected object"),
        }
    }
}
