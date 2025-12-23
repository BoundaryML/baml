//! Hover text extraction for inline tests.
//!
//! This module provides hover functionality without the full LSP infrastructure.

use baml_db::{
    RootDatabase,
    baml_hir::{self, ItemId, file_item_tree, project_items},
    baml_workspace::Project,
};

/// Get hover text for a symbol by name.
///
/// Looks up the symbol in the project and returns formatted hover text.
pub fn get_hover_for_symbol(db: &RootDatabase, root: Project, symbol_name: &str) -> Option<String> {
    let items = project_items(db, root);

    for item in items.items(db) {
        match item {
            ItemId::Function(func_loc) => {
                let file = func_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let func = &item_tree[func_loc.id(db)];

                if func.name.as_str() == symbol_name {
                    let sig = baml_hir::function_signature(db, *func_loc);
                    return Some(format_function_signature(&sig));
                }
            }
            ItemId::Class(class_loc) => {
                let file = class_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[class_loc.id(db)];

                if class.name.as_str() == symbol_name {
                    return Some(format_class_definition(class));
                }
            }
            ItemId::Enum(enum_loc) => {
                let file = enum_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[enum_loc.id(db)];

                if enum_def.name.as_str() == symbol_name {
                    return Some(format_enum_definition(enum_def));
                }
            }
            ItemId::TypeAlias(alias_loc) => {
                let file = alias_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[alias_loc.id(db)];

                if alias.name.as_str() == symbol_name {
                    return Some(format!(
                        "type {} = {}",
                        alias.name,
                        format_type_ref(&alias.type_ref)
                    ));
                }
            }
            ItemId::Client(client_loc) => {
                let file = client_loc.file(db);
                let item_tree = file_item_tree(db, file);
                let client = &item_tree[client_loc.id(db)];

                if client.name.as_str() == symbol_name {
                    return Some(format!(
                        "client {} {{\n  provider: {}\n}}",
                        client.name, client.provider
                    ));
                }
            }
            ItemId::Test(_) => {
                // Tests don't have hover
            }
        }
    }

    None
}

/// Format a function signature for hover display.
fn format_function_signature(sig: &baml_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, format_type_ref(&p.type_ref)))
        .collect();

    format!(
        "function {}({}) -> {}",
        sig.name,
        params.join(", "),
        format_type_ref(&sig.return_type)
    )
}

/// Format a TypeRef for display.
fn format_type_ref(ty: &baml_hir::TypeRef) -> String {
    use baml_db::baml_hir::TypeRef;

    match ty {
        TypeRef::Path(path) => path
            .segments
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("::"),
        TypeRef::Int => "int".to_string(),
        TypeRef::Float => "float".to_string(),
        TypeRef::String => "string".to_string(),
        TypeRef::Bool => "bool".to_string(),
        TypeRef::Null => "null".to_string(),
        TypeRef::Image => "image".to_string(),
        TypeRef::Audio => "audio".to_string(),
        TypeRef::Video => "video".to_string(),
        TypeRef::Pdf => "pdf".to_string(),
        TypeRef::Optional(inner) => format!("{}?", format_type_ref(inner)),
        TypeRef::List(inner) => format!("{}[]", format_type_ref(inner)),
        TypeRef::Map { key, value } => {
            format!("map<{}, {}>", format_type_ref(key), format_type_ref(value))
        }
        TypeRef::Union(types) => types
            .iter()
            .map(format_type_ref)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeRef::StringLiteral(s) => format!("\"{}\"", s),
        TypeRef::IntLiteral(i) => i.to_string(),
        TypeRef::FloatLiteral(f) => f.clone(),
        TypeRef::BoolLiteral(b) => b.to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(format_type_ref)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", format_type_ref(base), args_str)
        }
        TypeRef::TypeParam(name) => name.to_string(),
        TypeRef::Error => "<error>".to_string(),
        TypeRef::Unknown => "<unknown>".to_string(),
    }
}

/// Format a class definition for hover display.
fn format_class_definition(class: &baml_hir::Class) -> String {
    let mut lines = vec![format!("class {} {{", class.name)];

    for field in &class.fields {
        lines.push(format!(
            "  {} {}",
            field.name,
            format_type_ref(&field.type_ref)
        ));
    }

    lines.push("}".to_string());

    if class.is_dynamic {
        lines.push("// @@dynamic".to_string());
    }

    lines.join("\n")
}

/// Format an enum definition for hover display.
fn format_enum_definition(enum_def: &baml_hir::Enum) -> String {
    let mut lines = vec![format!("enum {} {{", enum_def.name)];

    for variant in &enum_def.variants {
        lines.push(format!("  {}", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use salsa::Setter;

    use super::*;

    #[test]
    fn test_hover_class() {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));

        let content = r#"class Person {
    name string
    age int
}"#;

        let source_file = db.add_file("test.baml", content);
        root.set_files(&mut db).to(vec![source_file]);

        let hover = get_hover_for_symbol(&db, root, "Person");
        assert!(hover.is_some());
        let hover_text = hover.unwrap();
        assert!(hover_text.contains("class Person"));
        assert!(hover_text.contains("name string"));
        assert!(hover_text.contains("age int"));
    }

    #[test]
    fn test_hover_function() {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));

        let content = r#"function Add(a: int, b: int) -> int {
    a + b
}"#;

        let source_file = db.add_file("test.baml", content);
        root.set_files(&mut db).to(vec![source_file]);

        let hover = get_hover_for_symbol(&db, root, "Add");
        assert!(hover.is_some());
        let hover_text = hover.unwrap();
        assert!(hover_text.contains("function Add"));
        assert!(hover_text.contains("a: int"));
        assert!(hover_text.contains("-> int"));
    }

    #[test]
    fn test_hover_enum() {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));

        let content = r#"enum Color {
    Red
    Green
    Blue
}"#;

        let source_file = db.add_file("test.baml", content);
        root.set_files(&mut db).to(vec![source_file]);

        let hover = get_hover_for_symbol(&db, root, "Color");
        assert!(hover.is_some());
        let hover_text = hover.unwrap();
        assert!(hover_text.contains("enum Color"));
        assert!(hover_text.contains("Red"));
        assert!(hover_text.contains("Green"));
        assert!(hover_text.contains("Blue"));
    }
}
