//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.

use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    file_item_tree,
    package::{PackageId, package_items},
};
use baml_compiler2_tir::package_interface::package_interface;
use baml_db::Name;

use crate::{
    db::ProjectDatabase,
    param_schema,
    param_schema::{ParamSchema, TypeSchema},
};

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
    /// Parameter schemas for the playground args form. Named types inside are
    /// [`crate::FieldSchema::Ref`]s into [`FunctionListing::types`]. `None`
    /// means no schema was extracted (function missing from the package
    /// interface mid-edit, or extraction skipped for companions/internal
    /// functions); `Some(vec![])` means the function takes no arguments.
    pub params: Option<Vec<ParamSchema>>,
}

/// Playground function metadata plus the shared type table their param
/// schemas reference into.
#[derive(Debug, Clone)]
pub struct FunctionListing {
    pub functions: Vec<FunctionSymbol>,
    /// Every named type referenced from any function's params, defined exactly
    /// once and keyed by canonical dotted FQN (`user.shapes.Foo`).
    pub types: std::collections::BTreeMap<String, TypeSchema>,
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

/// List user-facing functions with metadata for the playground, along with
/// the shared type table their param schemas reference.
///
/// Extracts LLM metadata (client name, `is_llm`) from `declarative_meta` on the
/// compiler2 [`Function`](baml_compiler2_hir::item_tree::Function) item tree entry.
pub fn list_functions_with_metadata(db: &ProjectDatabase) -> FunctionListing {
    let pkg_id = PackageId::new(db, Name::new("user"));
    let pkg = package_items(db, pkg_id);
    let iface = package_interface(db, pkg_id);
    let mut functions = Vec::new();
    let mut types = std::collections::BTreeMap::new();
    for (namespace_path, ns_items) in &pkg.namespaces {
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

                let origin: FunctionOrigin = func.origin.into();
                // Companions clone parent params verbatim and non-userDefined
                // functions are hidden by default — extracting schemas for
                // them only duplicates payload. The UI degrades to raw mode.
                let params = if is_sub_function || origin != FunctionOrigin::UserDefined {
                    None
                } else {
                    param_schema::function_param_schemas(
                        db,
                        iface,
                        namespace_path,
                        name,
                        is_llm,
                        &mut types,
                    )
                };

                functions.push(FunctionSymbol {
                    name: playground_function_name(namespace_path, name),
                    origin,
                    is_llm,
                    client_name,
                    is_sub_function,
                    params,
                });
            }
        }
    }
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    FunctionListing { functions, types }
}

/// Playground-qualified function names only — for callers like the CFG
/// snapshot that key by name and must not pay for schema extraction.
pub fn list_playground_function_names(db: &ProjectDatabase) -> Vec<String> {
    let pkg_id = PackageId::new(db, Name::new("user"));
    let pkg = package_items(db, pkg_id);
    let mut names = Vec::new();
    for (namespace_path, ns_items) in &pkg.namespaces {
        for (name, defn) in &ns_items.values {
            if defn.kind() == DefinitionKind::Function {
                names.push(playground_function_name(namespace_path, name));
            }
        }
    }
    names.sort();
    names
}

/// Function names exposed to the playground preserve source namespaces so the
/// UI can group them. Root-level functions keep their historical bare names.
pub(crate) fn playground_function_name(namespace_path: &[Name], name: &Name) -> String {
    if namespace_path.is_empty() {
        return name.to_string();
    }

    let mut parts = Vec::with_capacity(namespace_path.len() + 1);
    parts.extend(namespace_path.iter().map(ToString::to_string));
    parts.push(name.to_string());
    parts.join(".")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        db
    }

    #[test]
    fn playground_function_metadata_preserves_namespace_paths() {
        let mut db = make_db();
        db.add_or_update_file(
            std::path::Path::new("/tmp/main.baml"),
            "function RootMain() -> int { 1 }",
        );
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_demo/demo.baml"),
            "function DemoFunc() -> int { 2 }",
        );
        db.add_or_update_file(
            std::path::Path::new("/tmp/ns_demo/ns_inner/inner.baml"),
            "function InnerFunc() -> int { 3 }",
        );

        let names = list_functions_with_metadata(&db)
            .functions
            .into_iter()
            .map(|function| function.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "RootMain".to_string(),
                "demo.DemoFunc".to_string(),
                "demo.inner.InnerFunc".to_string(),
            ]
        );
    }
}
