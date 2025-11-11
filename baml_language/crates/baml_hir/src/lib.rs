//! High-level Intermediate Representation.
//!
//! Provides name resolution and semantic analysis after parsing.

use baml_base::{Name, SourceFile};
use baml_parser::syntax_tree;
use baml_workspace::project_files;

mod ids;
mod types;

pub use ids::*;
pub use types::*;

/// Tracked: get all items defined in a file
#[salsa::tracked]
pub fn file_items(db: &dyn salsa::Database, file: SourceFile) -> Vec<ItemId> {
    // TODO: Extract items from syntax tree
    let _tree = syntax_tree(db, file);
    vec![]
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
