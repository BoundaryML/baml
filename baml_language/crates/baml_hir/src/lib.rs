//! High-level Intermediate Representation.
//!
//! Provides name resolution and semantic analysis after parsing.

use baml_base::{Name, SourceFile};
use baml_parser::syntax_tree;
use baml_syntax::SyntaxKind;
use baml_workspace::project_files;

mod ids;
mod pretty_print;
mod types;

pub use ids::*;
pub use pretty_print::*;
pub use types::*;

/// Tracked: get all items defined in a file
#[salsa::tracked]
pub fn file_items(db: &dyn salsa::Database, file: SourceFile) -> Vec<ItemId> {
    let tree = syntax_tree(db, file);
    let file_id = file.file_id(db);
    let mut items = Vec::new();

    // Walk top-level nodes in the syntax tree
    // Note: syntax_tree returns a GreenNode, so we need to look at it correctly

    // Iterate children_with_tokens and filter to only SyntaxNodes
    for element in tree.children_with_tokens() {
        // Only process SyntaxNode elements (not tokens)
        if let Some(child) = element.into_node() {
            match child.kind() {
                SyntaxKind::FUNCTION_DEF => {
                    // Extract function name - WORD is a token, not a node
                    for element in child.children_with_tokens() {
                        if let Some(token) = element.into_token() {
                            if token.kind() == SyntaxKind::WORD {
                                let name_text = token.text().to_string();
                                let name = Name::new(&name_text);
                                items.push(ItemId::Function(FunctionId {
                                    file: file_id,
                                    name,
                                }));
                                break;
                            }
                        }
                    }
                }
                SyntaxKind::CLASS_DEF => {
                    // Extract class name - WORD is a token, not a node
                    for element in child.children_with_tokens() {
                        if let Some(token) = element.into_token() {
                            if token.kind() == SyntaxKind::WORD {
                                let name_text = token.text().to_string();
                                let name = Name::new(&name_text);
                                items.push(ItemId::Class(ClassId {
                                    file: file_id,
                                    name,
                                }));
                                break;
                            }
                        }
                    }
                }
                SyntaxKind::ENUM_DEF => {
                    // Extract enum name - WORD is a token, not a node
                    for element in child.children_with_tokens() {
                        if let Some(token) = element.into_token() {
                            if token.kind() == SyntaxKind::WORD {
                                let name_text = token.text().to_string();
                                let name = Name::new(&name_text);
                                items.push(ItemId::Enum(EnumId {
                                    file: file_id,
                                    name,
                                }));
                                break;
                            }
                        }
                    }
                }
                _ => {} // Skip other nodes
            }
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

/// Tracked: resolve a name to an item
#[salsa::tracked]
pub fn resolve_name(db: &dyn salsa::Database, from: SourceFile, _name: Name) -> Option<ItemId> {
    // TODO: Implement name resolution
    // For now, just check items in the current file
    let _items = file_items(db, from);

    // This is a stub - real implementation would check item names
    None
}

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

/// Helper to get function data (for compatibility)
pub fn function_data(_db: &dyn salsa::Database, _func: FunctionId) -> FunctionData {
    // TODO: Convert from tracked struct to data
    FunctionData {
        name: Name::new("stub"),
        params: vec![],
        return_type: TypeRef::Unknown,
    }
}

/// Helper to get class data (for compatibility)
pub fn class_data(_db: &dyn salsa::Database, _class: ClassId) -> ClassData {
    // TODO: Convert from tracked struct to data
    ClassData {
        name: Name::new("stub"),
        fields: vec![],
    }
}
