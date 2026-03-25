//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.
//!
//! NOTE: This module is a stub pending full compiler2 HIR symbol listing API.

use baml_db::Span;

/// The kind of a symbol in a BAML project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Enum,
    TypeAlias,
    Field,
    EnumVariant,
    Client,
    Test,
    Generator,
    TemplateString,
    RetryPolicy,
}

/// Information about a symbol in the project.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The name of the symbol.
    pub name: String,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// The file path containing the symbol.
    pub file_path: std::path::PathBuf,
    /// The span of the symbol in the source.
    pub span: Span,
}

use crate::ProjectDatabase;

/// List all functions in the project.
///
/// Returns all free functions in the "user" package by querying the compiler2 HIR.
pub fn list_functions(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    use baml_compiler2_hir::{
        contributions::DefinitionKind,
        package::{PackageId, package_items},
    };

    let pkg_id = PackageId::new(db, baml_db::Name::new("user"));
    let pkg_items = package_items(db, pkg_id);

    let mut symbols = Vec::new();
    for ns_items in pkg_items.namespaces.values() {
        for (name, defn) in &ns_items.values {
            if defn.kind() != DefinitionKind::Function {
                continue;
            }
            let file = defn.file(db);
            symbols.push(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Function,
                file_path: file.path(db).clone(),
                span: Span::default(),
            });
        }
    }
    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    symbols
}

/// List all classes in the project.
pub fn list_classes(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// List all enums in the project.
pub fn list_enums(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// List all type aliases in the project.
pub fn list_type_aliases(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// List all clients in the project.
pub fn list_clients(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// List all tests in the project.
pub fn list_tests(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// List all generators in the project.
pub fn list_generators(db: &ProjectDatabase, _project: baml_workspace::Project) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}

/// Find a symbol by name in the project.
pub fn find_symbol(
    db: &ProjectDatabase,
    project: baml_workspace::Project,
    name: &str,
) -> Option<Symbol> {
    find_symbol_locations(db, project, name).into_iter().next()
}

/// Find all locations where a symbol with the given name is defined.
pub fn find_symbol_locations(
    db: &ProjectDatabase,
    _project: baml_workspace::Project,
    _name: &str,
) -> Vec<Symbol> {
    let _ = db;
    Vec::new()
}
