use anyhow::Result;
use baml_types::CompletionState;

use super::ParseOptions;
use crate::jsonish::{
    parser::{
        fixing_parser,
        markdown_parser::{self, MarkdownResult},
        multi_json_parser,
    },
    value::Fixes,
    Value,
};

pub(super) fn parse_func(str: &str, mut options: ParseOptions, is_done: bool) -> Result<Value> {
    log::debug!("Parsing:\n{options:?}\n-------\n{str}\n-------");

    options.depth += 1;
    if options.depth > 100 {
        return Err(anyhow::anyhow!(
            "Depth limit reached. Likely a circular reference."
        ));
    }

    match serde_json::from_str(str) {
        Ok(mut v) => {
            match &mut v {
                Value::String(_, completion_state) => {
                    // The string must have been contained in quotes in order
                    // to parse as a JSON string, therefore it is complete.
                    *completion_state = CompletionState::Complete;
                }
                Value::Number(_, completion_state) => {
                    *completion_state = CompletionState::Incomplete;
                }
                Value::Boolean(_) => {}
                Value::Object(_, _) => {}
                Value::Array(_, _) => {}
                Value::Null => {}
                Value::Markdown(_, _, completion_state) => {
                    *completion_state = CompletionState::Incomplete;
                }
                Value::FixedJson(_, _) => {
                    unreachable!("Serde deserializes into concrete values, not FixedJson")
                }
                Value::AnyOf(_, _) => {
                    unreachable!("Serde deserializes into concrete values, not AnyOf")
                }
            }
            return Ok(Value::AnyOf(vec![v], str.to_string()));
        }
        Err(e) => {
            log::debug!("Invalid JSON: {e:?}");
        }
    };

    if options.allow_markdown_json {
        match markdown_parser::parse(str, &options) {
            Ok(items) => match items.len() {
                0 => {}
                1 => {
                    let res = items.into_iter().next();
                    match res {
                        Some(MarkdownResult::CodeBlock(s, v)) => {
                            return Ok(Value::AnyOf(
                                vec![Value::Markdown(
                                    s.to_string(),
                                    Box::new(v),
                                    CompletionState::Incomplete,
                                )],
                                str.to_string(),
                            ));
                        }
                        _ => {
                            log::debug!("Unexpected markdown result: {res:?}");
                        }
                    }
                }
                _ => {
                    // In the case of multiple JSON objects:
                    // Consider it as:
                    // [item1, item2, ..., itemN, [item1, item2, ..., itemN], str]
                    // AKA:
                    //  - All the items individually
                    //  - All the items as a list
                    //  - The original string

                    let others = items
                        .iter()
                        .filter_map(|res| match res {
                            MarkdownResult::String(s) => Some(s.as_str()),
                            _ => None,
                        })
                        .filter_map(|s| {
                            parse_func(
                                s,
                                options.next_from_mode(
                                    crate::jsonish::parser::ParsingMode::JsonMarkdownString,
                                ),
                                false,
                            )
                            .map_err(|e| {
                                log::debug!("Error parsing markdown string: {e:?}");
                                e
                            })
                            .ok()
                        })
                        .collect::<Vec<_>>();

                    let items = items
                        .into_iter()
                        .filter_map(|res| match res {
                            MarkdownResult::CodeBlock(s, v) => Some((s, v)),
                            _ => None,
                        })
                        .map(|(s, v)| {
                            Value::Markdown(
                                s.to_string(),
                                Box::new(v.clone()),
                                v.completion_state().clone(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let array = Value::Array(items.clone(), CompletionState::Incomplete);
                    let items = items
                        .into_iter()
                        .chain(std::iter::once(array))
                        .chain(others)
                        .collect::<Vec<_>>();
                    return Ok(Value::AnyOf(items, str.to_string()));
                }
            },
            Err(e) => {
                log::debug!("Markdown parsing error: {e:?}");
            }
        }
    }

    if options.all_finding_all_json_objects {
        match multi_json_parser::parse(str, &options) {
            Ok(mut items) => match items.len() {
                0 => {}
                1 => {
                    let first = items.pop().expect("Expected 1 item");
                    match &first {
                        // if the string is the same, then we can drop this condition.
                        Value::String(content, completion_state) if content == str => {}
                        _ => {
                            let ret = Value::AnyOf(
                                vec![Value::FixedJson(
                                    Box::new(first),
                                    vec![Fixes::GreppedForJSON],
                                )],
                                str.to_string(),
                            );
                            return Ok(ret);
                        }
                    }
                }
                n => {
                    let items_clone = Value::Array(items.clone(), CompletionState::Incomplete);
                    let items = items
                        .into_iter()
                        .chain(std::iter::once(items_clone))
                        .map(|v| Value::FixedJson(v.into(), vec![Fixes::GreppedForJSON]))
                        .collect::<Vec<_>>();
                    return Ok(Value::AnyOf(items, str.to_string()));
                }
            },
            Err(e) => {
                log::debug!("Error parsing multiple JSON objects: {e:?}");
            }
        }
    }

    if options.allow_fixes {
        match fixing_parser::parse(str, &options) {
            Ok(items) => {
                match items.len() {
                    0 => {}
                    1 => {
                        let (v, fixes) = items.into_iter().next().ok_or_else(|| {
                            anyhow::anyhow!("Expected 1 item when performing fixes")
                        })?;
                        // drop the fix if the string is the same
                        if fixes.is_empty()
                            && matches!(&v, Value::String(content, ..) if content == str)
                        {
                        } else {
                            return Ok(Value::AnyOf(
                                vec![Value::FixedJson(v.into(), fixes)],
                                str.to_string(),
                            ));
                        }
                    }
                    _ => {
                        // In the case of multiple JSON objects:
                        // Consider it as:
                        // [item1, item2, ..., itemN, [item1, item2, ..., itemN], str]
                        // AKA:
                        //  - All the items individually
                        //  - All the items as a list
                        //  - The original string

                        let items = items
                            .into_iter()
                            .map(|(v, fixes)| Value::FixedJson(v.into(), fixes))
                            .collect::<Vec<_>>();

                        let items_clone = Value::Array(items.clone(), CompletionState::Incomplete);

                        let items = items
                            .into_iter()
                            .chain(std::iter::once(items_clone))
                            .collect::<Vec<_>>();
                        return Ok(Value::AnyOf(items, str.to_string()));
                    }
                }
            }
            Err(e) => {
                log::debug!("Error fixing json: {e:?}");
            }
        }
    }

    if options.allow_as_string {
        return Ok(Value::String(
            str.to_string(),
            if is_done {
                CompletionState::Complete
            } else {
                CompletionState::Incomplete
            },
        ));
    }

    Err(anyhow::anyhow!("Failed to parse JSON"))
}

pub fn parse(str: &str, options: ParseOptions, is_done: bool) -> Result<Value> {
    let res = parse_func(str, options, is_done)?;
    Ok(res.simplify(is_done))
}

#[cfg(test)]
mod tests {
    use baml_types::CompletionState;

    use super::*;
    use crate::jsonish::Value;

    fn to_any_of(inner: Value, s: &str) -> Value {
        Value::AnyOf(vec![inner], s.to_string())
    }

    fn to_fixed(inner: Value, fixes: &[Fixes]) -> Value {
        Value::FixedJson(Box::new(inner), fixes.to_vec())
    }

    #[test]
    fn test_partial_int() {
        let res = parse_func("1", ParseOptions::default(), false).unwrap();
        assert_eq!(
            res,
            to_any_of(Value::Number(1.into(), CompletionState::Incomplete), "1")
        );
    }

    #[test]
    fn test_complete_list() {
        let res = parse_func("[1]", ParseOptions::default(), false).unwrap();
        assert_eq!(
            res,
            to_any_of(
                Value::Array(
                    vec![Value::Number(1.into(), CompletionState::Complete)],
                    CompletionState::Complete
                ),
                "[1]"
            )
        );
    }

    #[test]
    fn test_incomplete_list() {
        let res = parse_func("[1, 2", ParseOptions::default(), false).unwrap();
        assert_eq!(
            res,
            to_any_of(
                to_fixed(
                    to_any_of(
                        to_fixed(
                            Value::Array(
                                vec![
                                    Value::Number(1.into(), CompletionState::Complete),
                                    Value::Number(2.into(), CompletionState::Incomplete),
                                ],
                                CompletionState::Incomplete
                            ),
                            &[]
                        ),
                        "[1, 2"
                    ),
                    &[Fixes::GreppedForJSON]
                ),
                "[1, 2"
            )
        );
    }

    #[test]
    fn test_incomplete_nested_list() {
        let res = parse_func("[1, 2, [3", ParseOptions::default(), false).unwrap();
        assert_eq!(
            res,
            to_any_of(
                to_fixed(
                    to_any_of(
                        to_fixed(
                            Value::Array(
                                vec![
                                    Value::Number(1.into(), CompletionState::Complete),
                                    Value::Number(2.into(), CompletionState::Complete),
                                    Value::Array(
                                        vec![Value::Number(3.into(), CompletionState::Incomplete),],
                                        CompletionState::Incomplete
                                    )
                                ],
                                CompletionState::Incomplete
                            ),
                            &[]
                        ),
                        "[1, 2, [3"
                    ),
                    &[Fixes::GreppedForJSON]
                ),
                "[1, 2, [3"
            )
        );
    }

    #[test]
    fn test_markdown_multi_item_does_not_reparse_entire_input_as_string() {
        // When markdown parsing yields multiple items (e.g., multiple code blocks plus trailing text),
        // we should only parse the trailing text in JsonMarkdownString mode, not the full original input.
        //
        // Re-parsing the full input can create nested AnyOf structures that later leak via Display
        // (e.g. strings like `AnyOf[{,AnyOf[{,{},],]]i`).
        let input = r#"```json
{"a": 1}
```

```json
{"b": 2}
```

i"#;

        let res = parse_func(input, ParseOptions::default(), false).unwrap();
        let Value::AnyOf(choices, _) = res else {
            panic!("Expected AnyOf, got {res:#?}");
        };

        fn contains_trimmed_string(value: &Value, needle: &str) -> bool {
            match value {
                Value::String(s, _) => s.trim() == needle,
                Value::Object(kvs, _) => {
                    kvs.iter().any(|(_, v)| contains_trimmed_string(v, needle))
                }
                Value::Array(items, _) => items.iter().any(|v| contains_trimmed_string(v, needle)),
                Value::Markdown(_, v, _) => contains_trimmed_string(v, needle),
                Value::FixedJson(v, _) => contains_trimmed_string(v, needle),
                Value::AnyOf(choices, original) => {
                    original.trim() == needle
                        || choices.iter().any(|v| contains_trimmed_string(v, needle))
                }
                Value::Number(_, _) | Value::Boolean(_) | Value::Null => false,
            }
        }

        let contains_trailing_i = choices.iter().any(|v| contains_trimmed_string(v, "i"));
        assert!(
            contains_trailing_i,
            "Expected trailing markdown string candidate, got {choices:#?}"
        );

        assert!(
            !choices
                .iter()
                .any(|v| matches!(v, Value::AnyOf(_, original) if original == input)),
            "Unexpected re-parse of the full input in markdown string candidates: {choices:#?}"
        );
    }
}
