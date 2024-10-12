mod json_collection;
mod json_parse_state;

use crate::jsonish::{value::Fixes, Value};

use self::json_parse_state::JsonParseState;

use super::ParseOptions;
use anyhow::Result;

pub fn parse<'a>(str: &'a str, _options: &ParseOptions) -> Result<Vec<(Value, Vec<Fixes>)>> {
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
        eprintln!("main loop about to process_token {:?}", c);
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
        state.complete_collection();
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
                            .map(|f| Value::FixedJson(f.1.into(), f.2))
                            .collect(),
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
    use crate::jsonish::{Value, ParseOptions, Fixes};

    #[test]
    fn test_number() {
        let opts = ParseOptions::default();
        let vals = parse("12", &opts).unwrap();
        assert_eq!(vals.len(), 1);
        match vals[0].0.clone() {
            Value::Number(n) => {
                assert_eq!(n, 12.into())
            }
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_partial_array() {
        let opts = ParseOptions::default();
        let vals = parse("[12", &opts).unwrap();
        dbg!(&vals);
        assert_eq!(vals.len(), 1);
        match vals[0].0.clone() {
            Value::Number(n) => {
                assert_eq!(n, 1.into())
            }
            _ => panic!("Expected number"),
        }
    }
}
