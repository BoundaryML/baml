//! Hover information for BAML symbols.

use baml_db::{
    Name, SourceFile,
    baml_compiler_hir::{self, Db, ItemId, file_item_tree, project_items, type_ref_to_str},
    baml_workspace::Project,
};
use text_size::{TextRange, TextSize};

use crate::{MarkupKind, RangedValue};

/// Hover information for a symbol.
#[derive(Debug, Clone)]
pub struct Hover {
    contents: Vec<HoverContent>,
}

/// Content within a hover popup.
#[derive(Debug, Clone)]
pub enum HoverContent {
    /// A code signature (function, class, etc.).
    Signature(String),
    /// Documentation text.
    Docstring(String),
}

impl Hover {
    /// Create a new hover with signature content.
    pub fn with_signature(signature: String) -> Self {
        Self {
            contents: vec![HoverContent::Signature(signature)],
        }
    }

    /// Check if hover has any content.
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    /// Format the hover for display.
    pub fn display(&self, kind: MarkupKind) -> String {
        let mut result = String::new();
        for content in &self.contents {
            match content {
                HoverContent::Signature(sig) => match kind {
                    MarkupKind::PlainText => result.push_str(sig),
                    MarkupKind::Markdown => {
                        result.push_str("```baml\n");
                        result.push_str(sig);
                        result.push_str("\n```");
                    }
                },
                HoverContent::Docstring(doc) => {
                    if !result.is_empty() {
                        result.push_str("\n\n");
                    }
                    result.push_str(doc);
                }
            }
        }
        result
    }
}

/// Get hover information at the given position.
///
/// Returns `None` if there's nothing to show at this position.
pub fn hover(
    db: &dyn Db,
    file: SourceFile,
    project: Project,
    offset: TextSize,
) -> Option<RangedValue<Hover>> {
    let text = file.text(db);

    // Get the word at the cursor position
    let (word, word_range) = get_word_at_offset(text, offset)?;

    // Look up the symbol by name
    let hover_text = get_hover_text_for_symbol(db, project, &word)?;

    Some(RangedValue::new(
        word_range,
        Hover::with_signature(hover_text),
    ))
}

/// Extract the word (identifier) at the given byte offset.
fn get_word_at_offset(text: &str, offset: TextSize) -> Option<(String, TextRange)> {
    let offset_usize: usize = offset.into();

    if offset_usize > text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Find word start (scan backward)
    let mut start = offset_usize;
    while start > 0 {
        let prev = start - 1;
        if !is_identifier_char(bytes[prev]) {
            break;
        }
        start = prev;
    }

    // Find word end (scan forward)
    let mut end = offset_usize;
    while end < bytes.len() && is_identifier_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    let word = text[start..end].to_string();
    let range = TextRange::new(
        TextSize::from(to_u32_saturating(start)),
        TextSize::from(to_u32_saturating(end)),
    );

    Some((word, range))
}

fn is_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Get hover text for a symbol by name.
fn get_hover_text_for_symbol(db: &dyn Db, project: Project, name: &str) -> Option<String> {
    let name_to_find = Name::new(name);
    let items = project_items(db, project);

    for item in items.items(db) {
        match item {
            ItemId::Function(func_loc) => {
                let file = func_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let func = &item_tree[func_loc.id(db)];

                if func.name == name_to_find {
                    let sig = baml_compiler_hir::function_signature(db, *func_loc);
                    return Some(format_function_signature(&sig));
                }
            }
            ItemId::Class(class_loc) => {
                let file = class_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[class_loc.id(db)];

                if class.name == name_to_find {
                    return Some(format_class_definition(class));
                }
            }
            ItemId::Enum(enum_loc) => {
                let file = enum_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[enum_loc.id(db)];

                if enum_def.name == name_to_find {
                    return Some(format_enum_definition(enum_def));
                }
            }
            ItemId::TypeAlias(alias_loc) => {
                let file = alias_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[alias_loc.id(db)];

                if alias.name == name_to_find {
                    return Some(format!(
                        "type {} = {}",
                        alias.name,
                        type_ref_to_str(&alias.type_ref)
                    ));
                }
            }
            ItemId::Client(client_loc) => {
                let file = client_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let client = &item_tree[client_loc.id(db)];

                if client.name == name_to_find {
                    return Some(format!("client {}", client.name));
                }
            }
            ItemId::Generator(gen_loc) => {
                let file = gen_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let generator = &item_tree[gen_loc.id(db)];

                if generator.name == name_to_find {
                    return Some(format!("generator {}", generator.name));
                }
            }
            ItemId::Test(test_loc) => {
                let file = test_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let test = &item_tree[test_loc.id(db)];

                if test.name == name_to_find {
                    return Some(format!("test {}", test.name));
                }
            }
            ItemId::TemplateString(ts_loc) => {
                let file = ts_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let ts = &item_tree[ts_loc.id(db)];

                if ts.name == name_to_find {
                    let sig = baml_compiler_hir::template_string_signature(db, *ts_loc);
                    return Some(format_template_string_signature(&sig));
                }
            }
        }
    }

    None
}

/// Format a template string signature for hover display.
fn format_template_string_signature(sig: &baml_compiler_hir::TemplateStringSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, type_ref_to_str(&p.type_ref)))
        .collect();

    format!(
        "template_string {}({}) -> string",
        sig.name,
        params.join(", ")
    )
}

/// Format a function signature for hover display.
fn format_function_signature(sig: &baml_compiler_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, type_ref_to_str(&p.type_ref)))
        .collect();

    format!(
        "function {}({}) -> {}",
        sig.name,
        params.join(", "),
        type_ref_to_str(&sig.return_type)
    )
}

/// Format a class definition for hover display.
fn format_class_definition(class: &baml_compiler_hir::Class) -> String {
    let mut lines = vec![format!("class {} {{", class.name)];

    for field in &class.fields {
        lines.push(format!(
            "  {} {}",
            field.name,
            type_ref_to_str(&field.type_ref)
        ));
    }

    lines.push("}".to_string());

    if class.is_dynamic.is_explicit() {
        lines.push("// @@dynamic".to_string());
    }

    lines.join("\n")
}

/// Format an enum definition for hover display.
fn format_enum_definition(enum_def: &baml_compiler_hir::Enum) -> String {
    let mut lines = vec![format!("enum {} {{", enum_def.name)];

    for variant in &enum_def.variants {
        lines.push(format!("  {}", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

fn to_u32_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
