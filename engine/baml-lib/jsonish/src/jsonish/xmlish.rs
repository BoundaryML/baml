use anyhow::{bail, Result};
use baml_types::{CompletionState, TypeIR};
use internal_baml_core::ir::TypeValue;
use internal_baml_jinja::types::OutputFormatContent;
use quick_xml::{events::Event, Reader};

use super::Value;

const MAX_XML_DEPTH: usize = 100;

#[derive(Debug)]
struct XmlNode {
    name: String,
    text: String,
    children: Vec<XmlNode>,
    complete: bool,
}

pub(crate) fn parse(
    output_format: &OutputFormatContent,
    target: &TypeIR,
    input: &str,
    is_done: bool,
) -> Result<Value> {
    let roots = parse_nodes(input)?
        .into_iter()
        .filter(|node| root_matches_target(output_format, target, node))
        .collect::<Vec<_>>();
    let mut values = if roots.len() > 1 && matches!(target, TypeIR::List(_, _)) {
        vec![node_to_value(
            output_format,
            target,
            &XmlNode {
                name: "items".to_string(),
                text: String::new(),
                complete: roots.iter().all(|node| node.complete),
                children: roots,
            },
            is_done,
        )?]
    } else {
        roots
            .iter()
            .map(|node| node_to_value(output_format, target, node, is_done))
            .collect::<Result<Vec<_>>>()?
    };

    match values.len() {
        0 => bail!("No XML elements found"),
        1 => Ok(values.pop().expect("length checked")),
        _ => Ok(Value::AnyOf(values, input.to_string())),
    }
}

fn root_matches_target(
    output_format: &OutputFormatContent,
    target: &TypeIR,
    node: &XmlNode,
) -> bool {
    match target {
        TypeIR::Primitive(TypeValue::Null, _) => is_null_node(node),
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) => same_name(&node.name, "value"),
        TypeIR::Enum { name, .. } => {
            same_name(&node.name, "value")
                || output_format.find_enum(name).is_ok_and(|enm| {
                    same_name(&node.name, enm.name.real_name())
                        || same_name(&node.name, enm.name.rendered_name())
                })
        }
        TypeIR::Class { name, mode, .. } => {
            output_format.find_class(mode, name).is_ok_and(|class| {
                let matches_class = |candidate: &XmlNode| {
                    same_name(&candidate.name, class.name.real_name())
                        || same_name(&candidate.name, class.name.rendered_name())
                };
                matches_class(node)
                    || (node.children.len() == 1 && matches_class(&node.children[0]))
            })
        }
        TypeIR::List(item, _) => {
            same_name(&node.name, "items")
                || same_name(&node.name, "list")
                || same_name(&node.name, "array")
                || same_name(&node.name, "item")
                || class_list_wrapper_matches(output_format, item, node)
                || root_matches_target(output_format, item, node)
        }
        TypeIR::Map(_, _, _) => same_name(&node.name, "map"),
        TypeIR::Union(items, _) => {
            same_name(&node.name, "value")
                || is_null_node(node)
                || items
                    .iter_include_null()
                    .iter()
                    .any(|variant| root_matches_target(output_format, variant, node))
        }
        TypeIR::RecursiveTypeAlias { name, .. } => output_format
            .find_recursive_alias_target(name)
            .is_ok_and(|target| root_matches_target(output_format, target, node)),
        TypeIR::Tuple(_, _) | TypeIR::Arrow(_, _) | TypeIR::Top(_) => false,
    }
}

fn parse_nodes(input: &str) -> Result<Vec<XmlNode>> {
    let mut reader = Reader::from_str(input);
    reader.trim_text(false);
    // This lets us recover useful trees from a common LLM mistake such as
    // `<a><b>value</a>` as well as from missing closing tags at EOF.
    reader.check_end_names(false);

    let mut stack: Vec<XmlNode> = Vec::new();
    let mut roots = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                if stack.len() >= MAX_XML_DEPTH {
                    bail!("XML depth limit reached");
                }
                stack.push(XmlNode {
                    name: local_name(start.local_name().as_ref()),
                    text: String::new(),
                    children: Vec::new(),
                    complete: false,
                });
            }
            Ok(Event::Empty(start)) => attach_node(
                XmlNode {
                    name: local_name(start.local_name().as_ref()),
                    text: String::new(),
                    children: Vec::new(),
                    complete: true,
                },
                &mut stack,
                &mut roots,
            ),
            Ok(Event::End(end)) => {
                if let Some(mut node) = stack.pop() {
                    node.complete = normalized_name(&node.name)
                        == normalized_name(&local_name(end.local_name().as_ref()));
                    attach_node(node, &mut stack, &mut roots);
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(node) = stack.last_mut() {
                    let decoded =
                        text.unescape()
                            .map(|value| value.into_owned())
                            .or_else(|_| {
                                reader
                                    .decoder()
                                    .decode(text.as_ref())
                                    .map(|value| value.into_owned())
                            })?;
                    node.text.push_str(&decoded);
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(reader.decoder().decode(text.as_ref())?.as_ref());
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_)) => {}
            Err(error) => {
                log::debug!("Recovering partial XML after parser error: {error}");
                break;
            }
        }
    }

    // Preserve a partial tree instead of discarding it when the model omitted
    // one or more trailing close tags.
    while let Some(mut node) = stack.pop() {
        node.complete = false;
        attach_node(node, &mut stack, &mut roots);
    }

    if roots.is_empty() {
        bail!("No XML elements found");
    }

    Ok(roots)
}

fn attach_node(node: XmlNode, stack: &mut [XmlNode], roots: &mut Vec<XmlNode>) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        roots.push(node);
    }
}

fn node_to_value(
    output_format: &OutputFormatContent,
    target: &TypeIR,
    node: &XmlNode,
    is_done: bool,
) -> Result<Value> {
    let completion = completion_state(node, is_done);

    Ok(match target {
        TypeIR::Primitive(TypeValue::Null, _) => Value::Null,
        TypeIR::Primitive(_, _) | TypeIR::Literal(_, _) | TypeIR::Enum { .. } => {
            Value::String(node.text.trim().to_string(), completion)
        }
        TypeIR::Class { name, mode, .. } => {
            let class = output_format.find_class(mode, name)?;
            let expected_names = [class.name.real_name(), class.name.rendered_name()];

            if node.children.len() == 1
                && expected_names
                    .iter()
                    .any(|name| same_name(name, &node.children[0].name))
            {
                return node_to_value(output_format, target, &node.children[0], is_done);
            }

            let mut fields = Vec::with_capacity(node.children.len());
            for child in &node.children {
                if let Some((field_name, field_type, _, _)) = class.fields.iter().find(|field| {
                    same_name(field.0.real_name(), &child.name)
                        || same_name(field.0.rendered_name(), &child.name)
                }) {
                    fields.push((
                        field_name.rendered_name().to_string(),
                        node_to_value(output_format, field_type, child, is_done)?,
                    ));
                } else {
                    fields.push((child.name.clone(), node_to_untyped(child, is_done)));
                }
            }
            Value::Object(fields, completion)
        }
        TypeIR::List(item, _) => {
            // A nested list uses its outer `<item>` as a container for inner
            // `<item>` elements. An empty container represents an empty list.
            let is_nested_list = node.text.trim().is_empty()
                && node
                    .children
                    .iter()
                    .all(|child| same_name(&child.name, "item"));
            let class_list = class_list_wrapper_matches(output_format, item, node);
            let nested_class_list = node.children.len() == 1
                && class_list_wrapper_matches(output_format, item, &node.children[0]);
            let items = if class_list {
                node.children
                    .iter()
                    .map(|child| node_to_value(output_format, item, child, is_done))
                    .collect::<Result<Vec<_>>>()?
            } else if nested_class_list {
                node.children[0]
                    .children
                    .iter()
                    .map(|child| node_to_value(output_format, item, child, is_done))
                    .collect::<Result<Vec<_>>>()?
            } else if (same_name(&node.name, "item") && !is_nested_list)
                || (!is_generic_list_wrapper(node)
                    && root_matches_target(output_format, item, node))
            {
                vec![node_to_value(output_format, item, node, is_done)?]
            } else {
                node.children
                    .iter()
                    .map(|child| node_to_value(output_format, item, child, is_done))
                    .collect::<Result<Vec<_>>>()?
            };
            Value::Array(items, completion)
        }
        TypeIR::Map(_, value_type, _) => {
            let entries = if node
                .children
                .iter()
                .all(|child| same_name(&child.name, "entry"))
            {
                node.children
                    .iter()
                    .filter_map(|entry| {
                        let key = entry
                            .children
                            .iter()
                            .find(|child| same_name(&child.name, "key"))?;
                        let value = entry
                            .children
                            .iter()
                            .find(|child| same_name(&child.name, "value"))?;
                        Some((
                            key.text.trim().to_string(),
                            node_to_value(output_format, value_type, value, is_done),
                        ))
                    })
                    .map(|(key, value)| value.map(|value| (key, value)))
                    .collect::<Result<Vec<_>>>()?
            } else {
                node.children
                    .iter()
                    .map(|child| {
                        Ok((
                            child.name.clone(),
                            node_to_value(output_format, value_type, child, is_done)?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            Value::Object(entries, completion)
        }
        TypeIR::Union(_, _) if is_null_node(node) => Value::Null,
        TypeIR::Union(items, _) => {
            let variants = items.iter_include_null();
            if let Some(variant) = variants
                .iter()
                .find(|variant| root_matches_target(output_format, variant, node))
            {
                node_to_value(output_format, variant, node, is_done)?
            } else if node.children.len() == 1 {
                if let Some(variant) = variants
                    .iter()
                    .find(|variant| root_matches_target(output_format, variant, &node.children[0]))
                {
                    node_to_value(output_format, variant, &node.children[0], is_done)?
                } else {
                    node_to_untyped(node, is_done)
                }
            } else {
                node_to_untyped(node, is_done)
            }
        }
        TypeIR::RecursiveTypeAlias { name, .. } => node_to_value(
            output_format,
            output_format.find_recursive_alias_target(name)?,
            node,
            is_done,
        )?,
        TypeIR::Tuple(_, _) | TypeIR::Arrow(_, _) | TypeIR::Top(_) => {
            node_to_untyped(node, is_done)
        }
    })
}

fn node_to_untyped(node: &XmlNode, is_done: bool) -> Value {
    let completion = completion_state(node, is_done);
    if node.children.is_empty() {
        if is_null_node(node) {
            Value::Null
        } else {
            Value::String(node.text.trim().to_string(), completion)
        }
    } else if node
        .children
        .iter()
        .all(|child| same_name(&child.name, "item"))
        || (node.children.len() > 1
            && node
                .children
                .windows(2)
                .all(|pair| same_name(&pair[0].name, &pair[1].name)))
    {
        Value::Array(
            node.children
                .iter()
                .map(|child| node_to_untyped(child, is_done))
                .collect(),
            completion,
        )
    } else if node
        .children
        .iter()
        .all(|child| same_name(&child.name, "entry"))
    {
        let entries = node
            .children
            .iter()
            .filter_map(|entry| {
                let key = entry
                    .children
                    .iter()
                    .find(|child| same_name(&child.name, "key"))?;
                let value = entry
                    .children
                    .iter()
                    .find(|child| same_name(&child.name, "value"))?;
                Some((key.text.trim().to_string(), node_to_untyped(value, is_done)))
            })
            .collect();
        Value::Object(entries, completion)
    } else {
        Value::Object(
            node.children
                .iter()
                .map(|child| (child.name.clone(), node_to_untyped(child, is_done)))
                .collect(),
            completion,
        )
    }
}

fn completion_state(node: &XmlNode, is_done: bool) -> CompletionState {
    if is_done && node.complete {
        CompletionState::Complete
    } else {
        CompletionState::Incomplete
    }
}

fn is_null_node(node: &XmlNode) -> bool {
    same_name(&node.name, "null")
        || (node.children.is_empty() && node.text.trim().eq_ignore_ascii_case("null"))
}

fn same_name(left: &str, right: &str) -> bool {
    normalized_name(left) == normalized_name(right)
}

fn class_list_wrapper_matches(
    output_format: &OutputFormatContent,
    item: &TypeIR,
    node: &XmlNode,
) -> bool {
    match item {
        TypeIR::Class { name, mode, .. } => {
            output_format.find_class(mode, name).is_ok_and(|class| {
                same_name(&node.name, &format!("{}List", class.name.real_name()))
                    || same_name(&node.name, &format!("{}List", class.name.rendered_name()))
            })
        }
        TypeIR::RecursiveTypeAlias { name, .. } => output_format
            .find_recursive_alias_target(name)
            .is_ok_and(|target| class_list_wrapper_matches(output_format, target, node)),
        _ => false,
    }
}

fn is_generic_list_wrapper(node: &XmlNode) -> bool {
    same_name(&node.name, "items")
        || same_name(&node.name, "list")
        || same_name(&node.name, "array")
}

fn normalized_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn local_name(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_missing_close_tags() {
        let roots = parse_nodes("preamble <items><item>one</item><item>two").unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "items");
        assert_eq!(roots[0].children.len(), 2);
        assert!(!roots[0].complete);
    }

    #[test]
    fn ignores_preamble_and_decodes_text() {
        let roots = parse_nodes("Here is the result:\n<value>A &amp; B</value>").unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].text, "A & B");
    }

    #[test]
    fn rejects_excessive_nesting() {
        let input = format!(
            "{}value{}",
            "<item>".repeat(MAX_XML_DEPTH + 1),
            "</item>".repeat(MAX_XML_DEPTH + 1)
        );
        let error = parse_nodes(&input).unwrap_err();
        assert!(error.to_string().contains("depth limit"));
    }
}
