//! Pretty printing for HIR items.

use std::{fmt, fmt::Write};

use crate::{ClassId, EnumId, FunctionId, ItemId};

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemId::Function(func) => write!(f, "{func}"),
            ItemId::Class(class) => write!(f, "{class}"),
            ItemId::Enum(enum_) => write!(f, "{enum_}"),
        }
    }
}

impl fmt::Display for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "function {}", self.name.as_str())
    }
}

impl fmt::Display for ClassId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "class {}", self.name.as_str())
    }
}

impl fmt::Display for EnumId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "enum {}", self.name.as_str())
    }
}

/// Pretty print a list of items
pub fn format_items(items: &[ItemId]) -> String {
    let mut output = String::new();

    for item in items {
        write!(output, "  {item}").unwrap();
        // output.push_str(&format!("  {item}\n"));
    }

    if output.is_empty() {
        output.push_str("  (no items)\n");
    }

    output
}

/// Pretty print items grouped by type
pub fn format_items_grouped(items: &[ItemId]) -> String {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut enums = Vec::new();

    for item in items {
        match item {
            ItemId::Function(f) => functions.push(f),
            ItemId::Class(c) => classes.push(c),
            ItemId::Enum(e) => enums.push(e),
        }
    }

    let mut output = String::new();

    if !functions.is_empty() {
        writeln!(output, "Functions:").unwrap();
        for func in functions {
            writeln!(output, "  {func}").unwrap();
        }
        writeln!(output).unwrap();
    }

    if !classes.is_empty() {
        writeln!(output, "Classes:").unwrap();
        for class in classes {
            writeln!(output, "  {class}").unwrap();
        }
        writeln!(output).unwrap();
    }

    if !enums.is_empty() {
        writeln!(output, "Enums:").unwrap();
        for enum_ in enums {
            writeln!(output, "  {enum_}").unwrap();
        }
        writeln!(output).unwrap();
    }

    if output.is_empty() {
        writeln!(output, "(no items)").unwrap();
    }

    output
}
