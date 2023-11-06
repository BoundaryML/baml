use serde::{Deserialize, Serialize};

// Define Rust structs corresponding to the TypedDicts

#[derive(Serialize, Deserialize, Debug, Default)]
struct Meta {
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct EnumValue {
    name: String,
    meta: Meta,
}

#[derive(Serialize, Deserialize, Debug)]
struct EnumType {
    name: String,
    meta: Meta,
    values: Vec<EnumValue>,
}

// Utility functions similar to the Python version

fn as_comment(text: &str) -> String {
    text.lines()
        .map(|line| format!("/// {}", line.trim()))
        .collect::<Vec<String>>()
        .join("\n")
}

fn as_indented_string(content: &str, level: usize) -> String {
    let indentation = "    ".repeat(level);
    content
        .lines()
        .map(|line| format!("{}{}", indentation, line))
        .collect::<Vec<String>>()
        .join("\n")
}

// Specialized print functions adapted for Rust

fn print_enum_value(value: &EnumValue) -> String {
    match &value.meta.description {
        Some(description) => format!("{} // {}", value.name, description),
        None => value.name.clone(),
    }
}

pub(crate) fn print_enum(val: serde_json::Value) -> String {
    let enm: EnumType = serde_json::from_value(val).unwrap();
    let mut block = vec![];

    if let Some(description) = &enm.meta.description {
        block.push(as_comment(description));
    }

    block.push(format!("enum {} {{", enm.name));
    block.push("---".to_string());

    for value in &enm.values {
        block.push(as_indented_string(&print_enum_value(value), 1));
    }

    block.push("}".to_string());

    block.join("\n")
}
