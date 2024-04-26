use std::collections::HashSet;

use internal_baml_schema_ast::ast::TypeValue;

use crate::generate::{dir_writer::FileContent, ir::FieldType};

use super::ruby_language_features::ToRuby;

impl ToRuby for FieldType {
    fn to_ts(&self) -> String {
        match self {
            FieldType::Class(name) => name.clone(),
            FieldType::Enum(name) => name.clone(),
            FieldType::List(inner) => format!("{}[]", inner.to_ts()),
            FieldType::Map(key, value) => {
                format!("{{ [key: {}]: {} }}", key.to_ts(), value.to_ts())
            }
            FieldType::Primitive(r#type) => match r#type {
                TypeValue::Bool => "boolean".to_string(),
                TypeValue::Float => "number".to_string(),
                TypeValue::Int => "number".to_string(),
                TypeValue::String => "string".to_string(),
                TypeValue::Null => "null".to_string(),
                TypeValue::Char => "string".to_string(),
            },
            FieldType::Union(inner) => inner
                .iter()
                .map(|t| t.to_ts())
                .collect::<Vec<_>>()
                .join(" | "),
            FieldType::Tuple(inner) => format!(
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_ts())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("{} | null", inner.to_ts()),
        }
    }
}

pub(super) fn to_internal_type(r#type: &FieldType) -> String {
    match r#type {
        FieldType::Class(name) => format!("Internal{}", name),
        FieldType::Enum(name) => name.clone(),
        FieldType::List(inner) => format!("{}[]", to_internal_type(inner)),
        FieldType::Map(key, value) => {
            format!(
                "{{ [key: {}]: {} }}",
                to_internal_type(key),
                to_internal_type(value)
            )
        }
        FieldType::Primitive(r#type) => match r#type {
            TypeValue::Bool => "boolean".to_string(),
            TypeValue::Float => "number".to_string(),
            TypeValue::Int => "number".to_string(),
            TypeValue::String => "string".to_string(),
            TypeValue::Null => "null".to_string(),
            TypeValue::Char => "string".to_string(),
        },
        FieldType::Union(inner) => inner
            .iter()
            .map(|t| t.to_ts())
            .collect::<Vec<_>>()
            .join(" | "),
        FieldType::Tuple(inner) => format!(
            "[{}]",
            inner
                .iter()
                .map(|t| t.to_ts())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldType::Optional(inner) => format!("{} | null", inner.to_ts()),
    }
}

pub(super) fn to_internal_type_constructor(variable: &str, r#type: &FieldType) -> String {
    match r#type {
        FieldType::Class(name) => format!("new Internal{name}({variable})"),
        FieldType::Enum(_) => variable.to_string(),
        FieldType::List(inner) => format!(
            "{variable}.map(x => {})",
            to_internal_type_constructor("x", inner)
        ),
        FieldType::Map(_key, _value) => {
            unimplemented!("Map type is not supported in TypeScript")
        }
        FieldType::Primitive(_) => variable.to_string(),
        FieldType::Union(inner) => {
            let content = inner
                .iter()
                .map(|t| {
                    let response = to_internal_type_constructor("x", t);
                    format!(
                        r#"
if ({type_check}) {{
  return {response};
}}
                      "#,
                        type_check = to_type_check("x", t),
                    )
                    .trim()
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"
((x) => {{
{content}
}})({variable})
"#
            )
            .trim()
            .to_string()
        }
        FieldType::Tuple(inner) => format!(
            "[{}]",
            inner
                .iter()
                .enumerate()
                .map(|(i, t)| to_internal_type_constructor(&format!("{variable}[{i}]"), t))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldType::Optional(inner) => format!(
            "({variable} === null || {variable} === undefined) ? null : {}",
            to_internal_type_constructor(variable, inner)
        ),
    }
}

pub(super) fn to_type_check(variable: &str, r#type: &FieldType) -> String {
    match r#type {
        FieldType::Class(name) => format!("is{name}({variable})"),
        FieldType::Enum(name) => format!("is{name}({variable})"),
        FieldType::List(inner) => format!(
            "Array.isArray({variable}) && {variable}.every((x: any) => {})",
            to_type_check("x", inner)
        ),
        FieldType::Map(_key, _value) => {
            unimplemented!("Map type is not supported in TypeScript")
        }
        FieldType::Primitive(inner) => match inner {
            TypeValue::Bool => format!("typeof {variable} === 'boolean'"),
            TypeValue::Float => format!("typeof {variable} === 'number'"),
            TypeValue::Int => format!("typeof {variable} === 'number'"),
            TypeValue::String => format!("typeof {variable} === 'string'"),
            TypeValue::Null => format!("{variable} === null"),
            TypeValue::Char => format!("typeof {variable} === 'string'"),
        },
        FieldType::Union(inner) => inner
            .iter()
            .map(|t| {
                let response = to_type_check(variable, t);
                format!(r#"({response})"#,)
            })
            .collect::<Vec<_>>()
            .join(" || "),
        FieldType::Tuple(inner) => format!(
            "Array.isArray({variable}) && {variable}.length === {} && [{}]",
            inner.len(),
            inner
                .iter()
                .enumerate()
                .map(|(i, t)| to_type_check(&format!("{variable}[{i}]"), t))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldType::Optional(inner) => format!(
            "({variable} === null || {variable} === undefined) || {}",
            to_type_check(variable, inner)
        ),
    }
}

pub(super) fn walk_custom_types<'a>(r#type: &'a FieldType) -> impl Iterator<Item = &'a str> {
    let mut results = HashSet::new();

    // Recursive closure to walk through the types
    fn walk<'a>(r#type: &'a FieldType, results: &mut HashSet<&'a str>) {
        match r#type {
            FieldType::Union(types) | FieldType::Tuple(types) => {
                for t in types {
                    walk(t, results);
                }
            }
            FieldType::Class(name) | FieldType::Enum(name) => {
                results.insert(&name);
            }
            FieldType::List(inner) => walk(inner, results),
            FieldType::Map(_key, _value) => {
                // Handle or ignore the map type as needed
            }
            FieldType::Primitive(_) => (), // Ignore primitive types
            FieldType::Optional(inner) => walk(inner, results),
        }
    }

    // Start the recursive walk
    walk(r#type, &mut results);

    // Convert the results into an iterator
    results.into_iter()
}

pub(super) fn to_parse_expression(
    variable: &String,
    r#type: &FieldType,
    file: &mut FileContent,
) -> String {
    match r#type {
        FieldType::Class(name) => {
            file.add_import("../types_internal", format!("Internal{name}"), None, false);
            format!("Internal{name}.from({variable})")
        }
        FieldType::Enum(name) => format!("{variable} as {name}"),
        FieldType::List(inner) => {
            format!(
                "{variable}.map(x => {})",
                to_parse_expression(&"x".to_string(), inner, file)
            )
        }
        FieldType::Map(_key, _value) => {
            unimplemented!("Map type is not supported in TypeScript")
        }
        FieldType::Primitive(_) => variable.to_string(),
        FieldType::Union(inner) => {
            let content = inner
                .iter()
                .map(|t| {
                    let response = to_parse_expression(variable, t, file);
                    format!(
                        r#"
if (to_type_check({variable}, t)) {{
    return {response};
}}"#
                    )
                    .trim()
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"
((x) => {{
    {content}
    throw new Error(`Could not parse {variable} as {curr}`);
}})({variable})
"#,
                curr = to_internal_type(r#type),
            )
            .trim()
            .to_string()
        }
        FieldType::Tuple(inner) => format!(
            "[{}]",
            inner
                .iter()
                .enumerate()
                .map(|(i, t)| to_parse_expression(&format!("{variable}[{i}]"), t, file))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldType::Optional(inner) => format!(
            "({variable} === null || {variable} === undefined) ? null : {}",
            to_parse_expression(variable, inner, file)
        ),
    }
}
