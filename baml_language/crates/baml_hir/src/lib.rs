//! High-level Intermediate Representation.
//!
//! Provides name resolution and semantic analysis after parsing.

use std::sync::Arc;

use baml_base::{FileId, Name, SourceFile, Span};
use baml_parser::syntax_tree;
use baml_syntax::ast::Item;
use baml_workspace::project_files;
use rowan::ast::AstNode;

mod expr;
mod ids;
mod types;

pub use expr::*;
pub use ids::*;
pub use types::*;

// ============================================================================
// Item Queries
// ============================================================================

/// Tracked: get all items defined in a file
#[salsa::tracked]
pub fn file_items(db: &dyn salsa::Database, file: SourceFile) -> Vec<ItemId> {
    let tree = syntax_tree(db, file);

    let source_file = match baml_syntax::ast::SourceFile::cast(tree.clone()) {
        Some(sf) => sf,
        None => return vec![],
    };

    let mut items = Vec::new();

    for item in source_file.items() {
        match item {
            Item::Function(func) => {
                if let Some(name_token) = func.name() {
                    let name = Name::new(name_token.text());
                    items.push(ItemId::Function(FunctionId { file, name }));
                }
            }
            Item::Class(class) => {
                if let Some(name_token) = class.name() {
                    let name = Name::new(name_token.text());
                    items.push(ItemId::Class(ClassId { file, name }));
                }
            }
            Item::Enum(enum_def) => {
                if let Some(name_token) = enum_def.name() {
                    let name = Name::new(name_token.text());
                    items.push(ItemId::Enum(EnumId { file, name }));
                }
            }
            // Other items (Client, Test, etc.) are not tracked for now
            _ => {}
        }
    }

    items
}

/// Tracked: get all items in the entire project
#[salsa::tracked]
pub fn project_items(db: &dyn salsa::Database, root: baml_workspace::ProjectRoot) -> Vec<ItemId> {
    let files = project_files(db, root);
    let mut all_items = Vec::new();

    for file in files {
        let items = file_items(db, file);
        all_items.extend(items);
    }

    all_items
}

/// Tracked: resolve a name to an item in the project
#[salsa::tracked]
pub fn resolve_name(db: &dyn salsa::Database, from: SourceFile, name: Name) -> Option<ItemId> {
    // Check items in the current file first
    let items = file_items(db, from);

    for item in items {
        match &item {
            ItemId::Function(f) if f.name == name => return Some(item),
            ItemId::Class(c) if c.name == name => return Some(item),
            ItemId::Enum(e) if e.name == name => return Some(item),
            _ => {}
        }
    }

    None
}

// ============================================================================
// Tracked Struct Definitions (for future use)
// ============================================================================

/// Tracked struct for function definitions
#[salsa::tracked]
pub struct FunctionDef<'db> {
    pub name: Name,

    #[returns(ref)]
    pub params: Vec<Parameter>,

    pub return_type: TypeRef,
}

/// Tracked struct for class definitions
#[salsa::tracked]
pub struct ClassDef<'db> {
    pub name: Name,

    #[returns(ref)]
    pub fields: Vec<Field>,
}

// ============================================================================
// Data Access Functions
// ============================================================================

/// Get function data by looking up the function in the file's syntax tree.
///
/// This function is called on-demand to get function signature information.
pub fn function_data(db: &dyn salsa::Database, func: FunctionId) -> FunctionData {
    // Use the SourceFile stored in the FunctionId
    match function_data_from_file(db, func.file, &func.name) {
        Some(data) => data,
        None => FunctionData {
            name: func.name.clone(),
            params: vec![],
            return_type: TypeRef::Unknown,
        },
    }
}

/// Get function data from a SourceFile (more efficient version).
pub fn function_data_from_file(
    db: &dyn salsa::Database,
    file: SourceFile,
    func_name: &Name,
) -> Option<FunctionData> {
    let tree = syntax_tree(db, file);

    let source_file = baml_syntax::ast::SourceFile::cast(tree)?;

    for item in source_file.items() {
        if let Item::Function(func) = item {
            if let Some(name_token) = func.name() {
                if name_token.text() == func_name.as_str() {
                    let params = extract_parameters(&func);
                    let return_type = extract_return_type(&func);
                    return Some(FunctionData {
                        name: func_name.clone(),
                        params,
                        return_type,
                    });
                }
            }
        }
    }

    None
}

/// Get class data by looking up the class in the file's syntax tree.
pub fn class_data(db: &dyn salsa::Database, class: ClassId) -> ClassData {
    // Use the SourceFile stored in the ClassId
    match class_data_from_file(db, class.file, &class.name) {
        Some(data) => data,
        None => ClassData {
            name: class.name.clone(),
            fields: vec![],
        },
    }
}

/// Get class data from a SourceFile (more efficient version).
pub fn class_data_from_file(
    db: &dyn salsa::Database,
    file: SourceFile,
    class_name: &Name,
) -> Option<ClassData> {
    let tree = syntax_tree(db, file);

    let source_file = baml_syntax::ast::SourceFile::cast(tree)?;

    for item in source_file.items() {
        if let Item::Class(class) = item {
            if let Some(name_token) = class.name() {
                if name_token.text() == class_name.as_str() {
                    let fields = extract_class_fields(&class);
                    return Some(ClassData {
                        name: class_name.clone(),
                        fields,
                    });
                }
            }
        }
    }

    None
}

/// Get the body of a function.
pub fn function_body(db: &dyn salsa::Database, func: FunctionId) -> Arc<FunctionBody> {
    // Use the SourceFile stored in the FunctionId
    function_body_from_file(db, func.file, &func.name)
}

/// Get the body of a function from a SourceFile.
pub fn function_body_from_file(
    db: &dyn salsa::Database,
    file: SourceFile,
    func_name: &Name,
) -> Arc<FunctionBody> {
    let tree = syntax_tree(db, file);
    let file_id = file.file_id(db);

    let source_file = match baml_syntax::ast::SourceFile::cast(tree) {
        Some(sf) => sf,
        None => return Arc::new(FunctionBody::Missing),
    };

    for item in source_file.items() {
        if let Item::Function(func) = item {
            if let Some(name_token) = func.name() {
                if name_token.text() == func_name.as_str() {
                    return Arc::new(lower_function_body(&func, file_id));
                }
            }
        }
    }

    Arc::new(FunctionBody::Missing)
}

// ============================================================================
// AST Lowering Helpers
// ============================================================================

/// Extract parameters from a function definition.
fn extract_parameters(func: &baml_syntax::ast::FunctionDef) -> Vec<Parameter> {
    let mut params = Vec::new();

    if let Some(param_list) = func.param_list() {
        for param in param_list.params() {
            if let Some(name_token) = param.name() {
                let name = Name::new(name_token.text());
                let ty = param
                    .ty()
                    .map(|t| lower_type_expr(&t))
                    .unwrap_or(TypeRef::Unknown);
                params.push(Parameter { name, ty });
            }
        }
    }

    params
}

/// Extract return type from a function definition.
fn extract_return_type(func: &baml_syntax::ast::FunctionDef) -> TypeRef {
    func.return_type()
        .map(|t| lower_type_expr(&t))
        .unwrap_or(TypeRef::Unknown)
}

/// Extract fields from a class definition.
fn extract_class_fields(class: &baml_syntax::ast::ClassDef) -> Vec<Field> {
    let mut fields = Vec::new();

    for field in class.fields() {
        if let Some(name_token) = field.name() {
            let name = Name::new(name_token.text());
            let ty = field
                .ty()
                .map(|t| lower_type_expr(&t))
                .unwrap_or(TypeRef::Unknown);
            let optional = matches!(ty, TypeRef::Optional(_));
            fields.push(Field { name, ty, optional });
        }
    }

    fields
}

/// Lower a TypeExpr AST node to a TypeRef.
fn lower_type_expr(type_expr: &baml_syntax::ast::TypeExpr) -> TypeRef {
    let text = type_expr.syntax().text().to_string();
    parse_type_ref(&text)
}

/// Parse a type string into a TypeRef.
fn parse_type_ref(text: &str) -> TypeRef {
    let text = text.trim();

    // Handle optional types (T?)
    if text.ends_with('?') {
        let inner = parse_type_ref(&text[..text.len() - 1]);
        return TypeRef::Optional(Box::new(inner));
    }

    // Handle array types (T[])
    if text.ends_with("[]") {
        let inner = parse_type_ref(&text[..text.len() - 2]);
        return TypeRef::List(Box::new(inner));
    }

    // Handle union types (T1 | T2)
    if text.contains('|') {
        let parts: Vec<TypeRef> = text.split('|').map(|s| parse_type_ref(s.trim())).collect();
        return TypeRef::Union(parts);
    }

    // Named type
    TypeRef::Named(Name::new(text))
}

/// Lower a function body to HIR.
fn lower_function_body(func: &baml_syntax::ast::FunctionDef, file_id: FileId) -> FunctionBody {
    // Check if it's an LLM function
    if let Some(llm_body) = func.llm_body() {
        return FunctionBody::Llm(LlmBody {
            client: None,
            prompt: llm_body.syntax().text().to_string(),
        });
    }

    // Check if it's an expression function
    if let Some(expr_body) = func.expr_body() {
        let mut body = ExprBody::new();
        if let Some(root) = lower_expr_body(&expr_body, &mut body, file_id) {
            body.root_expr = Some(root);
        }
        return FunctionBody::Expr(Arc::new(body));
    }

    FunctionBody::Missing
}

/// Lower an expression function body to HIR expressions.
fn lower_expr_body(
    expr_body: &baml_syntax::ast::ExprFunctionBody,
    body: &mut ExprBody,
    file_id: FileId,
) -> Option<ExprId> {
    let syntax = expr_body.syntax();

    // For now, create a placeholder expression
    // Full implementation would recursively lower all expressions
    let span = Span::new(file_id, syntax.text_range());
    Some(body.alloc_expr(Expr::Missing, span))
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_db::RootDatabase;

    #[test]
    fn test_file_items_extraction() {
        let mut db = RootDatabase::new();
        let source = r##"
function HelloWorld(name: string) -> string {
  client GPT4
  prompt #"Say hello"#
}

class User {
  name string
  age int
}
"##;
        let file = db.add_file("test.baml", source);

        let tree = baml_parser::syntax_tree(&db, file);
        println!("Tree kind: {:?}", tree.kind());
        println!("Tree text length: {:?}", tree.text().len());

        // Try to cast
        let source_file = baml_syntax::ast::SourceFile::cast(tree.clone());
        println!("Cast successful: {}", source_file.is_some());

        if let Some(sf) = source_file {
            let items: Vec<_> = sf.items().collect();
            println!("Items found: {}", items.len());
            for item in &items {
                match item {
                    baml_syntax::ast::Item::Function(f) => {
                        println!("Function: {:?}", f.name().map(|t| t.text().to_string()));
                    }
                    baml_syntax::ast::Item::Class(c) => {
                        println!("Class: {:?}", c.name().map(|t| t.text().to_string()));
                    }
                    _ => println!("Other item type"),
                }
            }
        }

        let items = file_items(&db, file);
        println!("HIR items: {:?}", items);
        assert!(!items.is_empty(), "Should find items in the file");
    }
}
