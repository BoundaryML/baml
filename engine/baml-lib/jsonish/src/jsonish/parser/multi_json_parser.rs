use anyhow::Result;

use super::{entry, ParseOptions};
use crate::jsonish::Value;

pub fn parse(str: &str, options: &ParseOptions) -> Result<Vec<Value>> {
    // Find all balanced JSON objects but w/o any fixes.
    let mut stack = Vec::new();
    let mut json_str_start = None;
    let mut json_objects = Vec::new();

    for (index, character) in str.char_indices() {
        match character {
            '{' | '[' => {
                if stack.is_empty() {
                    json_str_start = Some(index);
                }
                stack.push(character);
            }
            '}' | ']' => {
                if let Some(last) = stack.last() {
                    let expected_open = if character == '}' { '{' } else { '[' };
                    if *last == expected_open {
                        stack.pop();
                    } else {
                        return Err(anyhow::anyhow!("Mismatched brackets"));
                    }
                }

                if stack.is_empty() {
                    let end_index = index + 1;
                    let json_str = if let Some(start) = json_str_start {
                        &str[start..end_index]
                    } else {
                        &str[..end_index]
                    };
                    match entry::parse_func(
                        json_str,
                        options.next_from_mode(super::ParsingMode::AllJsonObjects),
                        false,
                    ) {
                        Ok(json) => json_objects.push(json),
                        Err(e) => {
                            // Ignore errors
                            log::error!("Failed to parse JSON object: {e:?}");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        // We reached the end but the stack is not empty
        match json_str_start {
            Some(start) => {
                let json_str = &str[start..];
                match entry::parse_func(
                    json_str,
                    options.next_from_mode(super::ParsingMode::AllJsonObjects),
                    false,
                ) {
                    Ok(json) => {
                        complete_stack_head(&mut json_objects);
                        json_objects.push(json)
                    }
                    Err(e) => {
                        // Ignore errors
                        log::error!("Failed to parse JSON object: {e:?}");
                    }
                }
            }
            None => {
                log::error!("Unexpected state: stack is not empty but no JSON start was found");
            }
        }
    }

    match json_objects.len() {
        0 => Err(anyhow::anyhow!("No JSON objects found")),
        _ => Ok(json_objects),
    }
}

fn complete_stack_head(stack: &mut [Value]) {
    if let Some(v) = stack.last_mut() {
        v.complete_deeply();
    }
}

#[cfg(test)]
mod test {
    use baml_types::CompletionState;
    use test_log::test;

    use super::*;

    #[test]
    fn test_parse() -> Result<()> {
        let res = parse(
            r#"```json
{
    "a": 1
}
```

Also we've got a few more!
```python
print("Hello, world!")
```

```test json
["This is a test"]
```
"#,
            &ParseOptions::default(),
        );

        let res = res?;
        assert_eq!(res.len(), 2);
        {
            let value = &res[0];
            let Value::AnyOf(value, _) = value else {
                panic!("Expected AnyOf, got {value:#?}");
            };
            assert!(value.contains(&Value::Object(
                [(
                    "a".to_string(),
                    Value::Number((1).into(), CompletionState::Complete)
                )]
                .into_iter()
                .collect(),
                CompletionState::Complete
            )));
        }
        {
            let value = &res[1];
            let Value::AnyOf(value, _) = value else {
                panic!("Expected AnyOf, got {value:#?}");
            };
            assert!(value.contains(&Value::Array(
                vec![Value::String(
                    "This is a test".to_string(),
                    CompletionState::Complete
                )],
                CompletionState::Complete
            )));
        }

        Ok(())
    }
}
