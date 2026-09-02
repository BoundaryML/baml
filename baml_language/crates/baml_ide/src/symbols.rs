//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.

use baml_compiler2_hir::{contributions::Definition, package::package_items};
use baml_compiler2_hir_ty::package_interface::package_interface;
use baml_compiler2_ppir::item_data::{function_data, function_llm_meta, function_source_map};
use baml_db::{Name, ProjectDatabase};

use crate::{
    param_schema,
    param_schema::{ParamSchema, TypeSchema},
};

/// Whether an enumeration of the language surface should skip this
/// definition as synthesized.
///
/// Companions and auto-derives carry the
/// docstring of the declaration they shadow, so listing them makes every
/// original into several near-duplicate rows. Search and completion both
/// enumerate what a reader can write, so both ask here.
pub(crate) fn is_synthesized(
    db: &dyn baml_compiler2_ppir::Db,
    name: &Name,
    def: Definition<'_>,
) -> bool {
    if name.as_str().contains('$') {
        return true;
    }
    if let Definition::Function(func) = def {
        use baml_compiler2_ast::ast::FunctionOrigin;
        match function_data(db, func).metadata.origin {
            FunctionOrigin::UserDefined => false,
            FunctionOrigin::Companion | FunctionOrigin::Internal | FunctionOrigin::AutoDerive => {
                true
            }
        }
    } else {
        false
    }
}

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
    /// Source-like declaration including name, generic parameters, parameters,
    /// return type, and throws clause.
    pub signature: String,
    /// One-based source location of the function name.
    pub source_position: FunctionSourcePosition,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
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

/// List user-facing functions with metadata for the playground, along with
/// the shared type table their param schemas reference.
///
/// Extracts LLM metadata (client name, `is_llm`) from `declarative_meta` on the
/// compiler2 [`Function`](baml_compiler2_hir::item_tree::Function) item tree entry.
pub fn list_functions_with_metadata(db: &ProjectDatabase) -> FunctionListing {
    let pkg_id = baml_compiler2_hir::package::sole_workspace_package(db);
    let pkg = package_items(db, pkg_id);
    let iface = package_interface(db, pkg_id);
    let mut functions = Vec::new();
    let mut types = std::collections::BTreeMap::new();
    for (namespace_path, ns_items) in &pkg.namespaces {
        for (name, defn) in &ns_items.values {
            if let Definition::Function(func_loc) = defn {
                let llm_meta = function_llm_meta(db, *func_loc);
                let is_llm = llm_meta.is_some();
                let client_name = llm_meta
                    .as_ref()
                    .and_then(|meta| meta.client_name.as_ref())
                    .map(std::string::ToString::to_string);

                // Callable companions have names with `@` (e.g. `MyFunc@render_prompt`).
                let is_sub_function = name.as_str().contains('@');

                let function = function_data(db, *func_loc);
                let origin: FunctionOrigin = function.metadata.origin.into();
                // Companions clone parent params verbatim and non-userDefined
                // functions are hidden by default — extracting schemas for
                // them only duplicates payload. The UI degrades to raw mode.
                let params = if is_sub_function || origin != FunctionOrigin::UserDefined {
                    None
                } else {
                    param_schema::function_param_schemas(
                        db,
                        *func_loc,
                        iface,
                        namespace_path,
                        name,
                        is_llm,
                        &mut types,
                    )
                };

                functions.push(FunctionSymbol {
                    name: playground_function_name(namespace_path, name),
                    signature: render_function_signature(function),
                    source_position: function_source_position(db, *func_loc),
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

fn render_function_signature(function: &baml_compiler2_ppir::item_data::FunctionData) -> String {
    let generic_params = function
        .generic_params
        .iter()
        .map(|param| {
            if param.bounds.is_empty() {
                return param.name.to_string();
            }
            let bounds = param
                .bounds
                .iter()
                .map(|&id| function.type_refs.display(id).to_string())
                .collect::<Vec<_>>()
                .join(" & ");
            format!("{} extends {bounds}", param.name)
        })
        .collect::<Vec<_>>();
    let generics = if generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", generic_params.join(", "))
    };
    let params = function
        .params
        .iter()
        .map(|param| {
            let optional = if param.has_default { "?" } else { "" };
            match param.type_ref {
                Some(id) => format!(
                    "{}{optional}: {}",
                    param.name,
                    function.type_refs.display(id)
                ),
                None => param.name.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = function
        .return_type
        .map(|id| format!(" -> {}", function.type_refs.display(id)))
        .unwrap_or_default();
    let throws = function
        .throws
        .map(|id| format!(" throws {}", function.type_refs.display(id)))
        .unwrap_or_else(|| " throws never".to_string());
    format!(
        "function {}{generics}({params}){return_type}{throws}",
        function.name
    )
}

fn function_source_position(
    db: &ProjectDatabase,
    function: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> FunctionSourcePosition {
    let source_file = function.file(db);
    let source_map = function_source_map(db, function);
    let offset: u32 = if source_map.name_span.is_empty() {
        source_map.span.start()
    } else {
        source_map.name_span.start()
    }
    .into();
    let (line, character) = crate::line_index::LineIndex::new(source_file.text(db))
        .offset_to_position(offset)
        .unwrap_or_default();
    let path = source_file.path(db);
    let root = source_file.source_root(db);
    let relative_path = match root.kind(db) {
        baml_base::SourceRootKind::Workspace => path.strip_prefix(root.path(db)).unwrap_or(&path),
        baml_base::SourceRootKind::Stdlib
        | baml_base::SourceRootKind::Dependency
        | baml_base::SourceRootKind::Dynamic => &path,
    };

    FunctionSourcePosition {
        file: relative_path.to_string_lossy().into_owned(),
        line: line.saturating_add(1),
        column: character.saturating_add(1),
    }
}

/// [`playground_function_name`] with the namespace read from the file's
/// package info — the spelling every playground surface (CFG, cursor
/// context, run targets) uses for a function declared in `source_file`.
pub(crate) fn playground_function_name_for_file(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: baml_base::SourceFile,
    name: &Name,
) -> String {
    let package_info = baml_compiler2_hir::file_package::file_package(db, source_file);
    playground_function_name(&package_info.namespace_path, name)
}

/// Whether a declared function name answers to `target_name` in playground
/// addressing: the bare name, or the namespace-qualified playground name.
pub(crate) fn function_name_matches_source_name(
    db: &dyn baml_compiler2_ppir::Db,
    source_file: baml_base::SourceFile,
    name: &Name,
    target_name: &str,
) -> bool {
    name.as_str() == target_name
        || playground_function_name_for_file(db, source_file, name) == target_name
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDbExt;

    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("/tmp"));
        db
    }

    #[test]
    fn playground_function_metadata_preserves_namespace_paths() {
        let mut db = make_db();
        db.file(
            std::path::Path::new("/tmp/main.baml"),
            "function root_main() -> int { 1 }",
        );
        db.file(
            std::path::Path::new("/tmp/ns_demo/demo.baml"),
            "function demo_func() -> int { 2 }",
        );
        db.file(
            std::path::Path::new("/tmp/ns_demo/ns_inner/inner.baml"),
            "function inner_func() -> int { 3 }",
        );

        let names = list_functions_with_metadata(&db)
            .functions
            .into_iter()
            .map(|function| function.name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "demo.demo_func".to_string(),
                "demo.inner.inner_func".to_string(),
                "root_main".to_string(),
            ]
        );
    }

    #[test]
    fn playground_function_metadata_includes_signature_and_source_position() {
        let mut db = make_db();
        let root = std::path::Path::new("/tmp")
            .canonicalize()
            .unwrap_or_else(|_| "/tmp".into());
        db.file(
            &root.join("ns_demo/main.baml"),
            "\n\nfunction transform<T extends string>(value: T, count: int) -> T throws Error {\n  value\n}",
        );

        let functions = list_functions_with_metadata(&db).functions;
        let function = functions
            .iter()
            .find(|function| function.name == "demo.transform")
            .expect("transform should be listed");

        assert_eq!(
            function.signature,
            "function transform<T extends string>(value: T, count: int) -> T throws Error"
        );
        assert_eq!(
            function.source_position,
            FunctionSourcePosition {
                file: "ns_demo/main.baml".to_string(),
                line: 3,
                column: 10,
            }
        );
    }
}
