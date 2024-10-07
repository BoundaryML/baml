use anyhow::Result;

use crate::jsonish::{
    parser::{
        fixing_parser,
        markdown_parser::{self, MarkdownResult},
        multi_json_parser,
    },
    value::Fixes,
    Value,
};

use super::ParseOptions;

pub fn parse<'a>(str: &'a str, mut options: ParseOptions) -> Result<Value> {
    log::debug!("Parsing:\n{:?}\n-------\n{}\n-------", options, str);

    options.depth += 1;
    if options.depth > 100 {
        return Err(anyhow::anyhow!(
            "Depth limit reached. Likely a circular reference."
        ));
    }

    match serde_json::from_str(str) {
        Ok(v) => return Ok(Value::AnyOf(vec![v], str.to_string())),
        Err(e) => {
            log::debug!("Invalid JSON: {:?}", e);
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
                                vec![Value::Markdown(s.to_string(), Box::new(v))],
                                str.to_string(),
                            ));
                        }
                        _ => {
                            log::debug!("Unexpected markdown result: {:?}", res);
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
                            MarkdownResult::String(s) => Some(Value::String(s.to_string())),
                            _ => None,
                        })
                        .map(|v| {
                            parse(
                                str,
                                options.next_from_mode(
                                    crate::jsonish::parser::ParsingMode::JsonMarkdownString,
                                ),
                            )
                        })
                        .filter_map(|res| match res {
                            Ok(v) => Some(v),
                            Err(e) => {
                                log::debug!("Error parsing markdown string: {:?}", e);
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let items = items
                        .into_iter()
                        .filter_map(|res| match res {
                            MarkdownResult::CodeBlock(s, v) => Some((s, v)),
                            _ => None,
                        })
                        .map(|(s, v)| Value::Markdown(s.to_string(), Box::new(v)))
                        .collect::<Vec<_>>();
                    let array = Value::Array(items.clone());
                    let items = items
                        .into_iter()
                        .chain(std::iter::once(array))
                        .chain(others)
                        .collect::<Vec<_>>();
                    return Ok(Value::AnyOf(items, str.to_string()));
                }
            },
            Err(e) => {
                log::debug!("Markdown parsing error: {:?}", e);
            }
        }
    }

    if options.all_finding_all_json_objects {
        match multi_json_parser::parse(str, &options) {
            Ok(items) => match items.len() {
                0 => {}
                1 => {
                    return Ok(Value::AnyOf(
                        vec![Value::FixedJson(
                            items.into_iter().next().unwrap().into(),
                            vec![Fixes::GreppedForJSON],
                        )],
                        str.to_string(),
                    ))
                }
                _ => {
                    let items_clone = Value::Array(items.clone());
                    let items = items
                        .into_iter()
                        .chain(std::iter::once(items_clone))
                        .map(|v| Value::FixedJson(v.into(), vec![Fixes::GreppedForJSON]))
                        .collect::<Vec<_>>();
                    return Ok(Value::AnyOf(items, str.to_string()));
                }
            },
            Err(e) => {
                log::debug!("Error parsing multiple JSON objects: {:?}", e);
            }
        }
    }

    if options.allow_fixes {
        match fixing_parser::parse(str, &options) {
            Ok(items) => {
                match items.len() {
                    0 => {}
                    1 => {
                        let (v, fixes) = items.into_iter().next().unwrap();
                        return Ok(Value::AnyOf(
                            vec![Value::FixedJson(v.into(), fixes)],
                            str.to_string(),
                        ));
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

                        let items_clone = Value::Array(items.clone());

                        let items = items
                            .into_iter()
                            .chain(std::iter::once(items_clone))
                            .collect::<Vec<_>>();
                        return Ok(Value::AnyOf(items, str.to_string()));
                    }
                }
            }
            Err(e) => {
                log::debug!("Error fixing json: {:?}", e);
            }
        }
    }

    if options.allow_as_string {
        return Ok(Value::String(str.to_string()));
    }

    Err(anyhow::anyhow!("Failed to parse JSON"))
}
