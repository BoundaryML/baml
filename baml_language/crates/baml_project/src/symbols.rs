//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.

use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    file_item_tree,
    package::{PackageId, package_items},
};
use baml_db::Name;

use crate::db::ProjectDatabase;

/// Symbol kind — locally defined since v1 HIR is removed.
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
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: std::path::PathBuf,
    pub span: baml_db::Span,
}

/// Extended function metadata for the playground.
#[derive(Debug, Clone)]
pub struct FunctionSymbol {
    pub name: String,
    /// Whether the function came directly from user source, a companion, or compiler lowering.
    pub origin: FunctionOrigin,
    /// Whether this is an LLM function (has `client`/`prompt` declarative body).
    pub is_llm: bool,
    /// The LLM client name (if LLM function).
    pub client_name: Option<String>,
    /// Whether this function is compiler-generated (`render_prompt`, `build_request`, `resolve`).
    pub is_sub_function: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    AutoDerive,
}

impl From<baml_compiler2_ast::ast::FunctionOrigin> for FunctionOrigin {
    fn from(origin: baml_compiler2_ast::ast::FunctionOrigin) -> Self {
        match origin {
            baml_compiler2_ast::ast::FunctionOrigin::UserDefined => Self::UserDefined,
            baml_compiler2_ast::ast::FunctionOrigin::Companion => Self::Companion,
            baml_compiler2_ast::ast::FunctionOrigin::Internal => Self::Internal,
            baml_compiler2_ast::ast::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
}

/// Extended test metadata for the playground.
#[derive(Debug, Clone)]
pub struct TestSymbol {
    pub name: String,
    /// The first function this test targets.
    pub function_name: String,
    /// Test args serialized as a JSON string.
    pub args_json: String,
}

/// List all functions in the project.
pub fn list_functions(db: &ProjectDatabase) -> Vec<Symbol> {
    let pkg_id = PackageId::new(db, Name::new("user"));
    let pkg = package_items(db, pkg_id);
    let mut result = Vec::new();
    for ns_items in pkg.namespaces.values() {
        for (name, defn) in &ns_items.values {
            if defn.kind() == DefinitionKind::Function {
                result.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    file_path: defn.file(db).path(db).clone(),
                    span: baml_db::Span::default(),
                });
            }
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// List user-facing functions with metadata for the playground.
///
/// Extracts LLM metadata (client name, `is_llm`) from `declarative_meta` on the
/// compiler2 [`Function`](baml_compiler2_hir::item_tree::Function) item tree entry.
pub fn list_functions_with_metadata(db: &ProjectDatabase) -> Vec<FunctionSymbol> {
    let pkg_id = PackageId::new(db, Name::new("user"));
    let pkg = package_items(db, pkg_id);
    let mut result = Vec::new();
    for ns_items in pkg.namespaces.values() {
        for (name, defn) in &ns_items.values {
            if let Definition::Function(func_loc) = defn {
                let item_tree = file_item_tree(db, func_loc.file(db));
                let func = &item_tree[func_loc.id(db)];

                let is_llm = matches!(
                    func.declarative_meta,
                    Some(baml_compiler2_ast::ast::DeclarativeMeta::Llm(_))
                );
                let client_name =
                    if let Some(baml_compiler2_ast::ast::DeclarativeMeta::Llm(ref llm)) =
                        func.declarative_meta
                    {
                        llm.client.as_ref().map(std::string::ToString::to_string)
                    } else {
                        None
                    };

                // Sub-functions have names with '$' (e.g. MyFunc$render_prompt)
                let is_sub_function = name.as_str().contains('$');

                result.push(FunctionSymbol {
                    name: name.to_string(),
                    origin: func.origin.into(),
                    is_llm,
                    client_name,
                    is_sub_function,
                });
            }
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// List tests with full metadata for the playground.
pub fn list_tests_with_metadata(db: &ProjectDatabase) -> Vec<TestSymbol> {
    let pkg_id = PackageId::new(db, Name::new("user"));
    let pkg = package_items(db, pkg_id);
    let mut result = Vec::new();
    for ns_items in pkg.namespaces.values() {
        for (name, defn) in &ns_items.values {
            if let Definition::Test(test_loc) = defn {
                let item_tree = file_item_tree(db, test_loc.file(db));
                let test = &item_tree[test_loc.id(db)];

                // function_refs contains the function names this test targets
                let function_name = test
                    .function_refs
                    .first()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();

                // args parsing is skipped in canary's alloc_test — always empty for now
                let args_json = "{}".to_string();

                result.push(TestSymbol {
                    name: name.to_string(),
                    function_name,
                    args_json,
                });
            }
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

// --- Stubs for remaining symbol types (not yet ported to compiler2) ---

pub fn list_classes(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn list_enums(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn list_type_aliases(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn list_clients(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn list_tests(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn list_generators(_db: &ProjectDatabase) -> Vec<Symbol> {
    Vec::new()
}

pub fn find_symbol(db: &ProjectDatabase, name: &str) -> Option<Symbol> {
    find_symbol_locations(db, name).into_iter().next()
}

pub fn find_symbol_locations(_db: &ProjectDatabase, _name: &str) -> Vec<Symbol> {
    Vec::new()
}
