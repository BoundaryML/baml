use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

// Define Rust structs to represent the TypedDicts
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Meta {
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PrimitiveType {
    rtype: String, // In Rust, Literal types are not supported, so we'll use String
    optional: bool,
    value: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct EnumType {
    rtype: String,
    name: String,
    optional: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FieldType {
    name: String,
    meta: Option<Meta>,
    #[serde(rename = "type")]
    type_meta: Box<DataType>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ClassType {
    rtype: String,
    fields: Vec<FieldType>,
    optional: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListType {
    rtype: String,
    dims: i32,
    inner: Box<DataType>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct InlineType {
    rtype: String,
    #[serde(rename = "type")]
    type_meta: Box<DataType>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UnionType {
    rtype: String,
    members: Vec<DataType>,
}

// Use an enum to represent the union of types
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", untagged)]
enum DataType {
    Primitive(PrimitiveType),
    Class(ClassType),
    List(ListType),
    Inline(InlineType),
    Union(UnionType),
}

// Utility functions similar to the Python version
fn as_comment(text: &str) -> String {
    text.lines()
        .map(|line| format!("// {}", line.trim()))
        .collect::<Vec<String>>()
        .join("\n")
}

fn as_indented_string(content: &str, level: usize) -> String {
    let indentation = "  ".repeat(level);
    content
        .lines()
        .map(|line| format!("{}{}", indentation, line))
        .collect::<Vec<String>>()
        .join("\n")
}

fn print_optional(value: &str, is_optional: bool) -> String {
    if is_optional {
        format!("{} | null", value)
    } else {
        value.to_string()
    }
}

// The print functions will need to be adapted to Rust's ownership and borrowing rules

// Main function to print data type - this would need to be adapted to return a Rust String type
fn print_type(item: &DataType) -> String {
    match item {
        DataType::Primitive(p) => print_primitive(p),
        DataType::Class(c) => print_class(c),
        DataType::List(l) => print_list(l),
        DataType::Inline(i) => print_type(&*i.type_meta),
        DataType::Union(u) => print_union(u),
    }
}

fn print_primitive(item: &PrimitiveType) -> String {
    print_optional(&item.value, item.optional)
}

fn print_class(item: &ClassType) -> String {
    let fields: Vec<String> = item
        .fields
        .iter()
        .map(|field| {
            let description = field
                .meta
                .as_ref()
                .and_then(|m| m.description.clone())
                .unwrap_or_default();
            let comment = if !description.is_empty() {
                as_comment(&description) + "\n"
            } else {
                "".to_string()
            };
            let field_value = print_type(&field.type_meta);
            format!("{}pub {}: {}", comment, field.name, field_value)
        })
        .collect();

    let class_content = as_indented_string(&fields.join(",\n"), 1);
    let optional = item.optional.unwrap_or(false);
    print_optional(&format!("struct {{\n{}\n}}", class_content), optional)
}

fn print_list(item: &ListType) -> String {
    let inner_type = print_type(&*item.inner);
    format!("Vec<{}>", inner_type)
}

fn print_enum(item: &EnumType) -> String {
    print_optional(&format!("enum {}", item.name), item.optional)
}

fn print_union(item: &UnionType) -> String {
    let member_types: Vec<String> = item
        .members
        .iter()
        .map(|member| print_type(member))
        .collect();
    member_types.join(" | ")
}

pub(crate) fn print_entry(json_input: serde_json::Value) -> String {
    let parsed: DataType = serde_json::from_value(json_input).expect("Invalid JSON input");

    print_type(&parsed)
}
