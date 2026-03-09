//! High-level Intermediate Representation.
//!
//! Provides name resolution and semantic analysis after parsing.
//!
//! ## Architecture
//!
//! The HIR is built in layers:
//! 1. **`ItemTree`**: Position-independent item storage (signatures only)
//! 2. **Interning**: Locations → Stable IDs via Salsa
//! 3. **Name Resolution**: Paths → Item IDs (future)
//!
//! ## Key Design Choices
//!
//! - **Salsa-based incrementality**: Only recompute what changed
//! - **Stable IDs**: Content-based interning survives edits
//! - **Future-proof**: Ready for modules and generics

use std::sync::Arc;

use baml_base::{
    FieldAttr, FieldAttrInner, FileId, Name, SapAttrValue, SapConstValue, SourceFile, Span, TyAttr,
    TyAttrInner,
};
use baml_compiler_diagnostics::{HirDiagnostic, NameError};
use baml_compiler_parser::syntax_tree;
use baml_compiler_syntax::SyntaxNode;
use rowan::{SyntaxToken, TextRange, ast::AstNode};

// Module declarations
mod body;
mod client;
pub mod fqn;
mod generator;
mod generics;
mod ids;
mod item_tree;
mod loc;
mod path;
pub mod path_resolve;
pub mod pretty;
pub mod reserved_names;
mod signature;
mod source_map;
pub mod symbol_table;
mod test;
mod type_ref;

// Re-exports
pub use body::*;
pub use fqn::*;
pub use generics::*;
pub use ids::*;
pub use item_tree::*;
pub use loc::*;
pub use path::*;
pub use pretty::{body_to_code, type_ref_to_str};
pub(crate) use reserved_names::ReservedNamesMode;
// Re-export signature types explicitly (no wildcards to avoid conflicts)
pub use signature::{FunctionSignature, Param, TemplateStringSignature};
pub use source_map::{
    ErrorLocation, HirSourceMap, SignatureSourceMap, SpanResolutionContext, TirContext,
};
pub use symbol_table::*;
pub use type_ref::*;

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for HIR queries.
///
/// Extends `baml_compiler_ppir::Db` (which itself extends `baml_workspace::Db`).
/// Use the free functions in this crate (e.g., `project_items`, `file_items`)
/// for HIR queries.
#[salsa::db]
pub trait Db: baml_compiler_ppir::Db {}

//
// ───────────────────────────────────────────────────── TRACKED STRUCTS ─────
//

/// Tracked struct holding all items defined in a file.
///
/// This follows the Salsa 2022 pattern: instead of returning Vec<ItemId<'db>>
/// directly from a tracked function, we wrap it in a tracked struct with
/// #[returns(ref)] to avoid lifetime issues.
#[salsa::tracked]
pub struct FileItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub items: Vec<ItemId<'db>>,
}

/// Tracked struct holding all items in a project.
#[salsa::tracked]
pub struct ProjectItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub items: Vec<ItemId<'db>>,
}

/// Tracked struct holding the result of lowering a file.
///
/// Contains both the ItemTree and any diagnostics discovered during lowering.
/// This enables single-pass lowering with validation.
///
/// Note: `item_tree` is stored as `Arc<ItemTree>` so that `file_item_tree`
/// can return a cheap clone (Arc clone = reference count increment, O(1))
/// rather than cloning the entire ItemTree.
#[salsa::tracked]
pub struct LoweringResult<'db> {
    #[tracked]
    #[returns(ref)]
    pub item_tree: Arc<ItemTree>,

    #[tracked]
    #[returns(ref)]
    pub diagnostics: Vec<HirDiagnostic>,
}

//
// ──────────────────────────────────────────────────── LOWERING CONTEXT ─────
//

/// Context for lowering CST to HIR, accumulating diagnostics.
///
/// This follows the rust-analyzer pattern of collecting errors during traversal
/// rather than failing fast, allowing us to report all errors in one pass.
struct LoweringContext {
    /// File ID for creating spans.
    file_id: FileId,

    /// Accumulated diagnostics.
    diagnostics: Vec<HirDiagnostic>,
}

impl LoweringContext {
    fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            diagnostics: Vec::new(),
        }
    }

    /// Create a span for the given text range in this file.
    fn span(&self, range: TextRange) -> Span {
        Span::new(self.file_id, range)
    }

    /// Push a diagnostic to be reported.
    fn push_diagnostic(&mut self, diagnostic: HirDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Consume the context and return the collected diagnostics.
    fn finish(self) -> Vec<HirDiagnostic> {
        self.diagnostics
    }
}

//
// ────────────────────────────────────────────────────────── SALSA QUERIES ─────
//

/// Tracked: Lower a file's syntax tree to HIR, collecting diagnostics.
///
/// This is the primary lowering query that validates items during construction.
/// It returns both the `ItemTree` and any diagnostics discovered.
#[salsa::tracked]
pub fn file_lowering(db: &dyn Db, file: SourceFile) -> LoweringResult<'_> {
    let tree = syntax_tree(db, file);
    let file_id = file.file_id(db);
    let (item_tree, diagnostics) = lower_file_with_ctx(&tree, file_id);
    LoweringResult::new(db, Arc::new(item_tree), diagnostics)
}

/// Extract `ItemTree` from a file's syntax tree.
///
/// Works for both real and synthetic files: `syntax_tree` parses the file's
/// text, lowering produces the items. For real files, these are user-defined
/// items; for synthetic files, these are `stream_*` items.
#[salsa::tracked]
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    file_lowering(db, file).item_tree(db).clone()
}

// Future: When we add modules, we'll need a function like this:
// #[salsa::tracked]
// pub fn container_item_tree(db: &dyn Db, container: ContainerId) -> Arc<ItemTree>

/// Tracked: Get all items defined in a file.
///
/// Returns a tracked struct containing interned IDs for all top-level items,
/// including synthesized stream_* items from PPIR expansion.
#[salsa::tracked]
pub fn file_items(db: &dyn Db, file: SourceFile) -> FileItems<'_> {
    let item_tree = file_item_tree(db, file);
    let mut items = intern_all_items(db, file, &item_tree);

    // Also include synthesized stream_* items
    let synth = baml_compiler_ppir::ppir_expansion_cst(db, file);
    if let Some(synth_file) = synth.source_file(db) {
        let synth_tree = file_item_tree(db, synth_file);
        items.extend(intern_all_items(db, synth_file, &synth_tree));
    }

    FileItems::new(db, items)
}

/// Tracked: Get all items in the entire project.
#[salsa::tracked]
pub fn project_items(db: &dyn Db, root: baml_workspace::Project) -> ProjectItems<'_> {
    let mut all_items = Vec::new();

    for file in root.files(db) {
        all_items.extend(file_items(db, *file).items(db).iter().copied());
    }

    ProjectItems::new(db, all_items)
}

/// Tracked: Get generic parameters for a function.
///
/// This is queried separately from `ItemTree` for incrementality - changes to
/// generic parameters don't invalidate the `ItemTree`.
///
/// For now, this returns empty generic parameters since BAML doesn't currently
/// parse generic syntax. Future work will extract `<T>` from the CST.
#[salsa::tracked]
pub fn function_generic_params(_db: &dyn Db, _func: FunctionId<'_>) -> Arc<GenericParams> {
    // TODO: Extract generic parameters from CST when BAML adds generic syntax
    Arc::new(GenericParams::new())
}

/// Tracked: Get generic parameters for a class.
#[salsa::tracked]
pub fn class_generic_params(_db: &dyn Db, _class: ClassId<'_>) -> Arc<GenericParams> {
    // TODO: Extract generic parameters from CST when BAML adds generic syntax
    Arc::new(GenericParams::new())
}

/// Tracked: Get generic parameters for an enum.
#[salsa::tracked]
pub fn enum_generic_params(_db: &dyn Db, _enum: EnumId<'_>) -> Arc<GenericParams> {
    // TODO: Extract generic parameters from CST when BAML adds generic syntax
    Arc::new(GenericParams::new())
}

/// Tracked: Get generic parameters for a type alias.
#[salsa::tracked]
pub fn type_alias_generic_params(_db: &dyn Db, _alias: TypeAliasId<'_>) -> Arc<GenericParams> {
    // TODO: Extract generic parameters from CST when BAML adds generic syntax
    Arc::new(GenericParams::new())
}

//
// ────────────────────────────────────────────────── FUNCTION QUERIES ─────
//

/// Returns the signature of a function (params, return type, generics).
///
/// This is separate from the `ItemTree` to provide fine-grained incrementality.
/// Changing a function body does NOT invalidate this query.
///
/// This query returns only the position-independent signature data.
/// For source location information, use `function_signature_source_map`.
#[salsa::tracked]
pub fn function_signature<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> Arc<FunctionSignature> {
    let (signature, _source_map) = function_signature_with_source_map(db, function);
    signature
}

/// Returns the source map for a function signature (parameter and return type spans).
///
/// This is separate from `function_signature` to enable early cutoff:
/// when comments or whitespace change, `function_signature` can return
/// an equal value (cached), while this query returns updated spans.
#[salsa::tracked]
pub fn function_signature_source_map<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> SignatureSourceMap {
    let (_signature, source_map) = function_signature_with_source_map(db, function);
    source_map
}

/// The prefix used for builtin BAML files.
///
/// Files with paths starting with this prefix are treated as builtins
/// and their functions are namespaced accordingly.
pub const BUILTIN_PATH_PREFIX: &str = "<builtin>/";

/// Derive the namespace for a file based on its path.
///
/// Builtin files (paths starting with `<builtin>/`) get namespaced:
/// - `<builtin>/baml/llm.baml` → `Some(["baml", "llm"])`
/// - `<builtin>/baml/http.baml` → `Some(["baml", "http"])`
///
/// Regular user files return `None` (they're in the local namespace).
///
/// # Examples
///
/// ```ignore
/// // Builtin file "<builtin>/baml/llm.baml"
/// let ns = file_namespace(db, builtin_llm_file);
/// assert_eq!(ns, Some(Namespace::BamlStd { path: vec![Name::new("llm")] }));
///
/// // User file
/// let ns = file_namespace(db, user_file);
/// assert_eq!(ns, None);
/// ```
pub fn file_namespace(db: &dyn Db, file: SourceFile) -> Option<Namespace> {
    let path = file.path(db);
    let path_str = path.to_string_lossy();

    if !path_str.starts_with(BUILTIN_PATH_PREFIX) {
        return None;
    }

    // Extract path after prefix: "<builtin>/baml/llm.baml" -> "baml/llm.baml"
    let after_prefix = &path_str[BUILTIN_PATH_PREFIX.len()..];

    // Remove .baml extension and split by /
    let without_ext = after_prefix.strip_suffix(".baml").unwrap_or(after_prefix);
    let segments: Vec<Name> = without_ext.split('/').map(Name::new).collect();

    if segments.is_empty() {
        return None;
    }

    // Builtin files under "baml/" get BamlStd namespace with "baml" prefix stripped.
    // E.g., ["baml", "llm"] -> BamlStd { path: ["llm"] }
    if segments.first().is_some_and(|s| s.as_str() == "baml") {
        Some(Namespace::BamlStd {
            path: segments[1..].to_vec(),
        })
    } else {
        Some(Namespace::UserModule {
            module_path: segments,
        })
    }
}

/// Returns the qualified name of a function.
///
/// Combines the file's namespace with the function's local name to produce
/// a fully qualified name that can be used for resolution and lookup.
///
/// # Examples
///
/// ```ignore
/// // Function `render_prompt` in `<builtin>/baml/llm.baml`
/// // -> QualifiedName { namespace: BamlStd { path: ["llm"] }, name: "render_prompt" }
/// // -> displays as "baml.llm.render_prompt"
///
/// // Function `my_func` in regular user file
/// // -> QualifiedName { namespace: Local, name: "my_func" }
/// // -> displays as "my_func"
/// ```
#[salsa::tracked]
pub fn function_qualified_name<'db>(db: &'db dyn Db, function: FunctionLoc<'db>) -> QualifiedName {
    let file = function.file(db);
    let signature = function_signature(db, function);

    let namespace = file_namespace(db, file).unwrap_or(Namespace::Local);
    QualifiedName {
        namespace,
        name: signature.name.clone(),
    }
}

/// Returns the qualified name of a class.
///
/// Mirrors `function_qualified_name` — classes in builtin BAML files
/// get `baml.llm.*` names, user classes get local names.
#[salsa::tracked]
pub fn class_qualified_name<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> QualifiedName {
    let file = class.file(db);
    let item_tree = file_item_tree(db, file);
    let class_def = &item_tree[class.id(db)];

    let namespace = file_namespace(db, file).unwrap_or(Namespace::Local);
    QualifiedName {
        namespace,
        name: class_def.name.clone(),
    }
}

/// Returns the qualified name of an enum.
///
/// Mirrors `class_qualified_name` — enums in builtin BAML files
/// get `baml.llm.*` names, user enums get local names.
#[salsa::tracked]
pub fn enum_qualified_name<'db>(db: &'db dyn Db, enum_loc: EnumLoc<'db>) -> QualifiedName {
    let file = enum_loc.file(db);
    let item_tree = file_item_tree(db, file);
    let enum_def = &item_tree[enum_loc.id(db)];

    let namespace = file_namespace(db, file).unwrap_or(Namespace::Local);
    QualifiedName {
        namespace,
        name: enum_def.name.clone(),
    }
}

/// Returns the set of variant names for an enum.
///
/// Per-enum Salsa query — only invalidated when that enum's file changes.
/// Used by path resolution so that modifying one enum doesn't force
/// re-resolution of paths involving other enums.
#[salsa::tracked(returns(ref))]
pub fn enum_variant_names<'db>(
    db: &'db dyn Db,
    enum_loc: EnumLoc<'db>,
) -> rustc_hash::FxHashSet<Name> {
    let item_tree = file_item_tree(db, enum_loc.file(db));
    let enum_data = &item_tree[enum_loc.id(db)];
    enum_data.variants.iter().map(|v| v.name.clone()).collect()
}

/// Internal helper that computes both signature and source map together.
///
/// Both `function_signature` and `function_signature_source_map` delegate to this,
/// but Salsa's early cutoff means that downstream queries depending only on
/// `function_signature` won't re-execute when only spans change.
fn function_signature_with_source_map<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> (Arc<FunctionSignature>, SignatureSourceMap) {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    let func_name = func.name.clone();

    // Client resolve functions have synthetic signatures: no params, returns PrimitiveClient.
    if matches!(
        &func.compiler_generated,
        Some(item_tree::CompilerGenerated::ClientResolve { .. })
    ) {
        return (
            Arc::new(FunctionSignature {
                name: func_name,
                params: vec![],
                return_type: TypeRef::path(path::Path::new(vec![
                    Name::new("baml"),
                    Name::new("llm"),
                    Name::new("PrimitiveClient"),
                ])),
                throws: None,
            }),
            SignatureSourceMap::default(),
        );
    }

    // Compiler-generated LLM functions: params from base LLM function in CST, return type per variant.
    if let Some(ref cg) = func.compiler_generated {
        let (base_name, return_type_override) = match cg {
            item_tree::CompilerGenerated::LlmCall { base_name } => (base_name.clone(), None),
            item_tree::CompilerGenerated::LlmRenderPrompt { base_name } => (
                base_name.clone(),
                Some(TypeRef::path(path::Path::new(vec![
                    Name::new("baml"),
                    Name::new("llm"),
                    Name::new("PromptAst"),
                ]))),
            ),
            item_tree::CompilerGenerated::LlmBuildRequest { base_name } => (
                base_name.clone(),
                Some(TypeRef::path(path::Path::new(vec![
                    Name::new("baml"),
                    Name::new("http"),
                    Name::new("Request"),
                ]))),
            ),
            item_tree::CompilerGenerated::ClientResolve { .. } => {
                // Already handled above
                unreachable!("ClientResolve returned earlier")
            }
        };
        let tree = syntax_tree(db, file);
        let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();
        let (base_sig, base_source_map) = source_file
            .items()
            .find_map(|item| {
                if let baml_compiler_syntax::ast::Item::Function(f) = item {
                    if f.name().as_ref().map(rowan::SyntaxToken::text) == Some(base_name.as_str()) {
                        return Some(FunctionSignature::lower(&f));
                    }
                }
                None
            })
            .unwrap_or((
                Arc::new(FunctionSignature {
                    name: base_name.clone(),
                    params: vec![],
                    return_type: TypeRef::unknown(),
                    throws: None,
                }),
                SignatureSourceMap::default(),
            ));
        let return_type = return_type_override.unwrap_or_else(|| base_sig.return_type.clone());
        // Use base source map only for LlmCall so param/return type errors are reported once.
        // For render_prompt/build_request, skip so we don't duplicate the same diagnostic.
        let source_map = match cg {
            item_tree::CompilerGenerated::LlmCall { .. } => base_source_map,
            _ => SignatureSourceMap::default(),
        };
        return (
            Arc::new(FunctionSignature {
                name: func_name,
                params: base_sig.params.clone(),
                return_type,
                throws: base_sig.throws.clone(),
            }),
            source_map,
        );
    }

    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    let default_signature = (
        Arc::new(FunctionSignature {
            name: func.name.clone(),
            params: vec![],
            return_type: TypeRef::unknown(),
            throws: None,
        }),
        SignatureSourceMap::default(),
    );

    let function_def = source_file.items().find_map(|item| match item {
        baml_compiler_syntax::ast::Item::Function(func_node) => {
            let func_node_name = func_node.name();
            if func_node_name.as_ref()?.text() == func_name {
                Some(FunctionSignature::lower(&func_node))
            } else {
                None
            }
        }
        baml_compiler_syntax::ast::Item::Class(class_node) => {
            class_node.methods().find_map(|method| {
                let method_name = method.name()?;
                let class_name = class_node.name();
                let class_name_text = class_name.as_ref()?.text();
                // func_name is qualified (ClassName.methodName), so compare against that
                let qualified_method_name =
                    QualifiedName::local_method_from_str(class_name_text, method_name.text());
                if qualified_method_name.as_str() == func_name.as_str() {
                    let namespace = file_namespace(db, file).unwrap_or(baml_base::Namespace::Local);
                    let self_type_name = baml_base::QualifiedName {
                        namespace,
                        name: Name::new(class_name_text),
                    }
                    .display_name();
                    Some(lower_method_signature(&method, &func_name, &self_type_name))
                } else {
                    None
                }
            })
        }
        _ => None,
    });

    function_def.unwrap_or(default_signature)
}

/// Lower a method signature, replacing 'self' parameter with the class type.
fn lower_method_signature(
    method_node: &baml_compiler_syntax::ast::FunctionDef,
    method_name: &Name,
    self_type_name: &Name,
) -> (Arc<FunctionSignature>, SignatureSourceMap) {
    let mut source_map = SignatureSourceMap::new();

    // Extract parameters, replacing 'self' with the class type
    let mut params = Vec::new();
    if let Some(param_list) = method_node.param_list() {
        for param_node in param_list.params() {
            if let Some(name_token) = param_node.name() {
                let param_name = name_token.text();
                let type_node = param_node.ty();
                let type_ref = if param_name == "self" {
                    // 'self' gets the class type
                    TypeRef::named(self_type_name.clone())
                } else {
                    type_node
                        .as_ref()
                        .map(TypeRef::from_ast)
                        .unwrap_or_else(TypeRef::unknown)
                };

                // Store the spans in the source map
                source_map.push_param_span(Some(param_node.syntax().text_range()));
                source_map.push_param_type_span(type_node.map(|t| t.syntax().text_range()));

                params.push(Param {
                    name: Name::new(param_name),
                    type_ref,
                });
            }
        }
    }

    // Extract return type and its span
    let return_type_node = method_node.return_type();
    let return_type = return_type_node
        .as_ref()
        .map(TypeRef::from_ast)
        .unwrap_or_else(TypeRef::unknown);

    // Store return type span in source map
    if let Some(span) = return_type_node.map(|t| t.text_range()) {
        source_map.set_return_type_span(span);
    }

    let throws_clause = method_node.throws_clause();
    let throws = throws_clause
        .as_ref()
        .and_then(baml_compiler_syntax::ThrowsClause::type_expr)
        .map(|te| {
            source_map.set_throws_type_span(te.syntax().text_range());
            TypeRef::from_ast(&te)
        });

    (
        Arc::new(FunctionSignature {
            name: method_name.clone(),
            params,
            return_type,
            throws,
        }),
        source_map,
    )
}

/// Tracked struct holding the fields of a class.
///
/// This follows the Salsa 2022 pattern: we wrap the result in a tracked struct
/// to enable fine-grained incrementality.
#[salsa::tracked]
pub struct ClassFields<'db> {
    #[tracked]
    #[returns(ref)]
    pub fields: Vec<(Name, TypeRef)>,
}

/// Returns the fields of a class as (name, type) pairs.
///
/// This query provides access to class field information from HIR,
/// allowing downstream queries (like type checking) to depend on
/// specific class field data.
#[salsa::tracked]
pub fn class_fields<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> ClassFields<'db> {
    let file = class.file(db);
    let item_tree = file_item_tree(db, file);
    let class_data = &item_tree[class.id(db)];

    let fields: Vec<(Name, TypeRef)> = class_data
        .fields
        .iter()
        .map(|f| (f.name.clone(), f.type_ref.clone()))
        .collect();

    ClassFields::new(db, fields)
}

/// Tracked struct holding all class fields in a project.
///
/// Maps class names to their field definitions (as HIR TypeRefs).
#[salsa::tracked]
pub struct ProjectClassFields<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<(Name, Vec<(Name, TypeRef)>)>,
}

/// Returns all class fields in a project.
///
/// This aggregates class field information across all files in the project,
/// providing a single query point for type checking.
#[salsa::tracked]
pub fn project_class_fields(db: &dyn Db, root: baml_workspace::Project) -> ProjectClassFields<'_> {
    let items = project_items(db, root);
    let mut classes = Vec::new();

    for item in items.items(db) {
        if let ItemId::Class(class_loc) = item {
            let class_fields_data = class_fields(db, *class_loc);
            let fields = class_fields_data.fields(db).clone();

            // Get the class name from the item tree
            let file = class_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let class_data = &item_tree[class_loc.id(db)];

            classes.push((class_data.name.clone(), fields));
        }
    }

    ProjectClassFields::new(db, classes)
}

/// Tracked struct holding all known type names in a project.
///
/// This includes classes, enums, and type aliases - any name that can be
/// used in a type position.
#[salsa::tracked]
pub struct ProjectTypeNames<'db> {
    #[tracked]
    #[returns(ref)]
    pub names: Vec<Name>,
}

/// Returns all known type names in a project.
///
/// This includes classes, enums, and type aliases. Used during type lowering
/// to validate that named types actually exist.
#[salsa::tracked]
pub fn project_type_names(db: &dyn Db, root: baml_workspace::Project) -> ProjectTypeNames<'_> {
    let items = project_items(db, root);
    let mut names = Vec::new();

    for item in items.items(db) {
        match item {
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                names.push(class.name.clone());
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];
                names.push(enum_def.name.clone());
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                names.push(alias.name.clone());
            }
            _ => {}
        }
    }

    ProjectTypeNames::new(db, names)
}

/// Returns a map of type item names to their spans.
///
/// This is a cached query that provides efficient Name -> Span lookups for
/// type-level error reporting (type aliases and classes). Used by `ErrorLocation::to_span`
/// to resolve `TypeItem(Name)` locations during diagnostic rendering.
///
/// Note: This query recomputes when file contents change (including whitespace),
/// since spans must be extracted from the syntax tree. The incrementality benefit
/// comes from storing `ErrorLocation::TypeItem(Name)` in type errors instead of
/// spans directly - type checking results remain cached even when this query invalidates.
#[salsa::tracked]
pub fn project_type_item_spans(
    db: &dyn Db,
    root: baml_workspace::Project,
) -> std::sync::Arc<std::collections::HashMap<Name, Span>> {
    let items = project_items(db, root);
    let mut spans = std::collections::HashMap::new();

    for item in items.items(db) {
        match item {
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                let name = class.name.clone();

                if let Some(span) = get_item_name_span(db, file, "class", &name, loc.id(db).index())
                {
                    spans.insert(name, span);
                }
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                let name = alias.name.clone();

                if let Some(span) =
                    get_item_name_span(db, file, "type alias", &name, loc.id(db).index())
                {
                    spans.insert(name, span);
                }
            }
            _ => {}
        }
    }

    std::sync::Arc::new(spans)
}

/// Returns class field type spans for error location resolution.
///
/// Maps (`class_name`, `field_index`) to the span of the field's type annotation.
/// Used by `ErrorLocation::ClassFieldType` to resolve to accurate spans.
pub fn project_class_field_type_spans(
    db: &dyn Db,
    root: baml_workspace::Project,
) -> std::sync::Arc<std::collections::HashMap<(Name, usize), Span>> {
    let items = project_items(db, root);
    let mut spans = std::collections::HashMap::new();

    for item in items.items(db) {
        if let ItemId::Class(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let class = &item_tree[loc.id(db)];
            let class_name = class.name.clone();

            // Get the CST to find field type spans
            let tree = syntax_tree(db, file);
            let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();
            let file_id = file.file_id(db);

            // Find the class in the CST
            if let Some(class_node) = source_file.items().find_map(|item| {
                if let baml_compiler_syntax::ast::Item::Class(c) = item {
                    if c.name().as_ref().map(SyntaxToken::text) == Some(class_name.as_str()) {
                        return Some(c);
                    }
                }
                None
            }) {
                // Extract field type spans
                for (field_index, field) in class_node.fields().enumerate() {
                    if let Some(type_expr) = field.ty() {
                        let range = type_expr.syntax().text_range();
                        let span = Span::new(file_id, range);
                        spans.insert((class_name.clone(), field_index), span);
                    }
                }
            }
        }
    }

    std::sync::Arc::new(spans)
}

/// Returns type alias type spans for error location resolution.
///
/// Maps (`alias_name`, `path`) to the span of a specific type within the alias's RHS.
/// The path navigates nested type constructors:
/// - For List: index 0 is the element type
/// - For Map: index 0 is the key type, index 1 is the value type
/// - For Union: index is the variant number (0, 1, 2, ...)
/// - For Optional: index 0 is the inner type
/// - Empty path means the entire RHS type expression
///
/// Used by `ErrorLocation::TypeAliasType` to resolve to accurate spans.
pub fn project_type_alias_type_spans(
    db: &dyn Db,
    root: baml_workspace::Project,
) -> std::sync::Arc<std::collections::HashMap<(Name, Vec<usize>), Span>> {
    let items = project_items(db, root);
    let mut spans = std::collections::HashMap::new();

    for item in items.items(db) {
        if let ItemId::TypeAlias(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let alias = &item_tree[loc.id(db)];
            let alias_name = alias.name.clone();

            // Get the CST to find type spans
            let tree = syntax_tree(db, file);
            let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();
            let file_id = file.file_id(db);

            // Find the type alias in the CST
            if let Some(alias_node) = source_file.items().find_map(|item| {
                if let baml_compiler_syntax::ast::Item::TypeAlias(a) = item {
                    if a.name().as_ref().map(SyntaxToken::text) == Some(alias_name.as_str()) {
                        return Some(a);
                    }
                }
                None
            }) {
                // Get the RHS type expression
                if let Some(type_expr) = alias_node.ty() {
                    // Collect spans for all paths within this type expression
                    collect_type_expr_spans(
                        &type_expr,
                        file_id,
                        &alias_name,
                        &mut vec![],
                        &mut spans,
                    );
                }
            }
        }
    }

    std::sync::Arc::new(spans)
}

/// Recursively collect spans for all paths within a type expression.
fn collect_type_expr_spans(
    type_expr: &baml_compiler_syntax::ast::TypeExpr,
    file_id: FileId,
    alias_name: &Name,
    current_path: &mut Vec<usize>,
    spans: &mut std::collections::HashMap<(Name, Vec<usize>), Span>,
) {
    use rowan::ast::AstNode;

    // Record the span for the current path
    let range = type_expr.syntax().text_range();
    let span = Span::new(file_id, range);
    spans.insert((alias_name.clone(), current_path.clone()), span);

    // Check if this is a union type
    if type_expr.is_union() {
        // For unions, use union_member_parts() to get each member's tokens/nodes
        for (i, member) in type_expr.union_member_parts().iter().enumerate() {
            current_path.push(i);

            // Record the span for this union member
            if let Some(range) = compute_union_member_range(member) {
                let span = Span::new(file_id, range);
                spans.insert((alias_name.clone(), current_path.clone()), span);
            }

            // If the member has a nested TYPE_EXPR (e.g., parenthesized type), recurse into it
            if let Some(inner_type_expr) = member.type_expr() {
                collect_type_expr_spans(&inner_type_expr, file_id, alias_name, current_path, spans);
            }

            // If the member has TYPE_ARGS (e.g., map<K, V>), recurse into those
            if let Some(type_args_node) = member.type_args() {
                let type_arg_exprs: Vec<_> = type_args_node
                    .children()
                    .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                    .collect();
                for (j, arg_node) in type_arg_exprs.iter().enumerate() {
                    if let Some(arg_type_expr) =
                        baml_compiler_syntax::ast::TypeExpr::cast(arg_node.clone())
                    {
                        current_path.push(j);
                        collect_type_expr_spans(
                            &arg_type_expr,
                            file_id,
                            alias_name,
                            current_path,
                            spans,
                        );
                        current_path.pop();
                    }
                }
            }

            current_path.pop();
        }
        return;
    }

    // Check if this is an optional type (trailing ?)
    if type_expr.is_optional() {
        // The inner type is the same node without the ? modifier
        // We need to find the base type - for simple cases, use child type exprs
        // For now, record index 0 for the inner part
        // Note: This is a simplification; the actual inner type span might need refinement
        if let Some(inner) = type_expr.inner_type_expr() {
            current_path.push(0);
            collect_type_expr_spans(&inner, file_id, alias_name, current_path, spans);
            current_path.pop();
        }
        return;
    }

    // Check if this is an array type (trailing [])
    if type_expr.is_array() {
        // Similar to optional, the element type needs to be found
        // For now, use inner_type_expr or child_type_exprs
        if let Some(inner) = type_expr.inner_type_expr() {
            current_path.push(0);
            collect_type_expr_spans(&inner, file_id, alias_name, current_path, spans);
            current_path.pop();
        }
        return;
    }

    // Check for generic types like map<K, V>
    let type_args = type_expr.type_arg_exprs();
    if !type_args.is_empty() {
        for (i, arg) in type_args.iter().enumerate() {
            current_path.push(i);
            collect_type_expr_spans(arg, file_id, alias_name, current_path, spans);
            current_path.pop();
        }
        return;
    }

    // Check for parenthesized types
    if type_expr.is_parenthesized() {
        if let Some(inner) = type_expr.inner_type_expr() {
            // Don't add to path for parentheses - they're just grouping
            collect_type_expr_spans(&inner, file_id, alias_name, current_path, spans);
        }
    }

    // For simple named types, function types, etc., we already recorded the span above
}

/// Compute the text range of a union member from its tokens and child nodes.
fn compute_union_member_range(
    member: &baml_compiler_syntax::ast::UnionMemberParts,
) -> Option<TextRange> {
    let mut start: Option<rowan::TextSize> = None;
    let mut end: Option<rowan::TextSize> = None;

    // Consider all tokens
    for token in &member.tokens {
        let range = token.text_range();
        match start {
            None => start = Some(range.start()),
            Some(s) if range.start() < s => start = Some(range.start()),
            _ => {}
        }
        match end {
            None => end = Some(range.end()),
            Some(e) if range.end() > e => end = Some(range.end()),
            _ => {}
        }
    }

    // Consider all child nodes
    for node in &member.child_nodes {
        let range = node.text_range();
        match start {
            None => start = Some(range.start()),
            Some(s) if range.start() < s => start = Some(range.start()),
            _ => {}
        }
        match end {
            None => end = Some(range.end()),
            Some(e) if range.end() > e => end = Some(range.end()),
            _ => {}
        }
    }

    match (start, end) {
        (Some(s), Some(e)) => Some(TextRange::new(s, e)),
        _ => None,
    }
}

/// Returns the file offset of a template string's raw string literal.
///
/// This is used at diagnostic rendering time to convert relative Jinja error
/// positions to absolute file positions, without storing the offset in cached data.
pub fn template_string_file_offset<'db>(
    db: &'db dyn Db,
    template_string: TemplateStringLoc<'db>,
) -> Option<u32> {
    let file = template_string.file(db);
    let item_tree = file_item_tree(db, file);
    let ts = &item_tree[template_string.id(db)];
    let ts_name = &ts.name;

    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree)?;

    for item in source_file.items() {
        if let baml_compiler_syntax::ast::Item::TemplateString(ts_node) = item {
            if ts_node.name().as_ref().map(SyntaxToken::text) == Some(ts_name) {
                if let Some(raw_string) = ts_node.raw_string() {
                    return Some(PromptTemplate::get_file_offset(&raw_string));
                }
            }
        }
    }
    None
}

/// Returns the file offset of an LLM function's prompt raw string literal.
///
/// This is used at diagnostic rendering time to convert relative Jinja error
/// positions to absolute file positions, without storing the offset in cached data.
pub fn llm_function_file_offset<'db>(db: &'db dyn Db, function: FunctionLoc<'db>) -> Option<u32> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    let func_name = &func.name;

    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree)?;

    // Find the function in the CST
    for item in source_file.items() {
        match item {
            baml_compiler_syntax::ast::Item::Function(func_node) => {
                if func_node.name().as_ref().map(SyntaxToken::text) == Some(func_name) {
                    if let Some(llm_body) = func_node.llm_body() {
                        if let Some(prompt_field) = llm_body.prompt_field() {
                            if let Some(raw_string) = prompt_field.raw_string() {
                                return Some(PromptTemplate::get_file_offset(&raw_string));
                            }
                        }
                    }
                }
            }
            baml_compiler_syntax::ast::Item::Class(class_node) => {
                // Also check class methods
                for method in class_node.methods() {
                    if let (Some(method_name), Some(class_name_token)) =
                        (method.name(), class_node.name())
                    {
                        let qualified = QualifiedName::local_method_from_str(
                            class_name_token.text(),
                            method_name.text(),
                        );
                        if qualified.as_str() == func_name.as_str() {
                            if let Some(llm_body) = method.llm_body() {
                                if let Some(prompt_field) = llm_body.prompt_field() {
                                    if let Some(raw_string) = prompt_field.raw_string() {
                                        return Some(PromptTemplate::get_file_offset(&raw_string));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns the names and spans of all functions defined in the project.
///
/// This is a convenience function for WASM/external consumers that just need
/// a list of function names without dealing with HIR internals.
/// Returns (`function_name`, span) pairs for `CodeLens` positioning.
pub fn list_function_names(db: &dyn Db, root: baml_workspace::Project) -> Vec<(String, Span)> {
    let items = project_items(db, root);
    let mut functions = Vec::new();

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let file = func_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[func_loc.id(db)];
            let func_name = func.name.clone();

            // Get the span from the CST
            let tree = syntax_tree(db, file);
            let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();
            let file_id = file.file_id(db);

            // Find the function in the CST to get its name span.
            // For compiler-generated LLM helpers (Foo.render_prompt, Foo.build_request), use the base LLM function's span.
            let name_to_find = match &func.compiler_generated {
                Some(
                    item_tree::CompilerGenerated::LlmRenderPrompt { base_name }
                    | item_tree::CompilerGenerated::LlmBuildRequest { base_name },
                ) => base_name.clone(),
                _ => func_name.clone(),
            };
            let span = source_file
                .items()
                .flat_map(|item| match item {
                    baml_compiler_syntax::ast::Item::Function(func_node) => vec![func_node],
                    baml_compiler_syntax::ast::Item::Class(class_node) => {
                        class_node.methods().collect()
                    }
                    _ => vec![],
                })
                .find(|function_def| {
                    function_def.name().as_ref().map(SyntaxToken::text)
                        == Some(name_to_find.as_str())
                })
                .and_then(|f| f.name())
                .map(|name_token| Span::new(file_id, name_token.text_range()))
                .unwrap_or_else(|| Span::new(file_id, TextRange::empty(0.into())));

            functions.push((func_name.to_string(), span));
        }
    }

    functions
}

/// The kind of a symbol in a BAML project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Enum,
    TypeAlias,
    Client,
    Test,
    Generator,
    TemplateString,
    RetryPolicy,
    /// A field within a class.
    Field,
    /// A variant within an enum.
    EnumVariant,
}

/// A symbol with proper CST ranges, suitable for document-symbol / outline views.
#[derive(Debug, Clone)]
pub struct FileSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: TextRange,
    pub selection_range: TextRange,
    pub children: Vec<FileSymbol>,
}

/// Returns all top-level symbols in a single file with accurate CST ranges.
///
/// Used by `textDocument/documentSymbol` to power the Outline view and `@`
/// symbol search.  Each symbol carries its full range and name-selection range
/// derived directly from the concrete syntax tree, so cursor-position matching
/// works correctly.
pub fn list_file_symbols(db: &dyn Db, file: SourceFile) -> Vec<FileSymbol> {
    use baml_compiler_syntax::ast;
    use rowan::ast::AstNode as _;

    let tree = syntax_tree(db, file);
    let Some(source_file) = ast::SourceFile::cast(tree) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();

    for item in source_file.items() {
        match item {
            ast::Item::Function(func) => {
                if let Some(name_token) = func.name() {
                    // Skip compiler-generated functions (render_prompt, build_request)
                    if name_token.text().contains('.') {
                        continue;
                    }
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::Function,
                        range: func.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
            ast::Item::Class(class) => {
                if let Some(name_token) = class.name() {
                    let children: Vec<FileSymbol> = class
                        .fields()
                        .filter_map(|field| {
                            let field_name = field.name()?;
                            Some(FileSymbol {
                                name: field_name.text().to_string(),
                                kind: SymbolKind::Field,
                                range: field.syntax().text_range(),
                                selection_range: field_name.text_range(),
                                children: Vec::new(),
                            })
                        })
                        .collect();

                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::Class,
                        range: class.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children,
                    });
                }
            }
            ast::Item::Enum(enum_def) => {
                if let Some(name_token) = enum_def.name() {
                    let children: Vec<FileSymbol> = enum_def
                        .variants()
                        .filter_map(|variant| {
                            let variant_name = variant.name()?;
                            Some(FileSymbol {
                                name: variant_name.text().to_string(),
                                kind: SymbolKind::EnumVariant,
                                range: variant.syntax().text_range(),
                                selection_range: variant_name.text_range(),
                                children: Vec::new(),
                            })
                        })
                        .collect();

                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::Enum,
                        range: enum_def.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children,
                    });
                }
            }
            ast::Item::TypeAlias(alias) => {
                if let Some(name_token) = alias.name() {
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::TypeAlias,
                        range: alias.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
            ast::Item::Client(client) => {
                if let Some(name_token) = client.name() {
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::Client,
                        range: client.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
            ast::Item::Test(test) => {
                if let Some(name_token) = test.name() {
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::Test,
                        range: test.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
            ast::Item::RetryPolicy(rp) => {
                if let Some(name_token) = rp.name() {
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::RetryPolicy,
                        range: rp.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
            ast::Item::TemplateString(ts) => {
                if let Some(name_token) = ts.name() {
                    symbols.push(FileSymbol {
                        name: name_token.text().to_string(),
                        kind: SymbolKind::TemplateString,
                        range: ts.syntax().text_range(),
                        selection_range: name_token.text_range(),
                        children: Vec::new(),
                    });
                }
            }
        }
    }

    symbols
}

/// Returns the body of a function (LLM prompt or expression IR).
///
/// This is the most frequently invalidated query - it changes whenever
/// the function body is edited.
///
/// TODO: It seems slow, iterating over all the functions every time you want to find one.
/// Can't we keep a hash map from `FunctionLoc` to `FunctionBody`?
#[salsa::tracked]
pub fn function_body<'db>(db: &'db dyn Db, function: FunctionLoc<'db>) -> Arc<FunctionBody> {
    Arc::new(build_function_body(db, function))
}

/// Collect all known type names (primitives + user-defined classes, enums, type aliases)
/// from the project for BEP-010 bare-type pattern sugar.
fn collect_known_type_names(db: &dyn Db) -> std::collections::HashSet<String> {
    let mut names: std::collections::HashSet<String> = body::PRIMITIVE_TYPE_NAMES
        .iter()
        .map(ToString::to_string)
        .collect();

    let project = db.project();
    for file in project.files(db) {
        let item_tree = file_item_tree(db, *file);
        for class in item_tree.classes.values() {
            names.insert(class.name.to_string());
        }
        for enum_def in item_tree.enums.values() {
            names.insert(enum_def.name.to_string());
        }
        for alias in item_tree.type_aliases.values() {
            names.insert(alias.name.to_string());
        }
    }
    names
}

/// Build a function body (pure syntactic lowering, no name resolution).
fn build_function_body<'db>(db: &'db dyn Db, function: FunctionLoc<'db>) -> FunctionBody {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    let func_name = func.name.clone();

    // Check if this is a compiler-generated client resolve function
    if let Some(item_tree::CompilerGenerated::ClientResolve { client_name }) =
        &func.compiler_generated
    {
        // Find the corresponding client definition in the source tree
        let tree = syntax_tree(db, file);
        let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

        let client_def = source_file.items().find_map(|item| {
            if let baml_compiler_syntax::ast::Item::Client(c) = item {
                if c.name()
                    .map(|n| n.text() == client_name.as_str())
                    .unwrap_or(false)
                {
                    return Some(c);
                }
            }
            None
        });

        if let Some(client) = client_def {
            // Get client metadata from item_tree
            let item_tree = file_item_tree(db, file);
            let client_data = item_tree.clients.values().find(|c| c.name == *client_name);

            let (provider, default_role, allowed_roles) = if let Some(c) = client_data {
                let allowed_roles = if c.allowed_roles.is_empty() {
                    vec![
                        "system".to_string(),
                        "user".to_string(),
                        "assistant".to_string(),
                    ]
                } else {
                    c.allowed_roles.clone()
                };
                let default_role = c.default_role.clone().unwrap_or_else(|| {
                    allowed_roles
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "user".to_string())
                });
                (c.provider.as_str().to_string(), default_role, allowed_roles)
            } else {
                (
                    "unknown".to_string(),
                    "user".to_string(),
                    vec![
                        "system".to_string(),
                        "user".to_string(),
                        "assistant".to_string(),
                    ],
                )
            };

            // Find the options block within the client's config
            if let Some(config_block) = client.config_block() {
                if let Some(options_item) = config_block.items().find(|i| i.matches_key("options"))
                {
                    if let Some(options_block) = options_item.nested_block() {
                        let file_id = file.file_id(db);
                        let (body, source_map) =
                            FunctionBody::lower_client_options_to_primitive_client(
                                &options_block,
                                file_id,
                                client_name.as_str(),
                                &provider,
                                &default_role,
                                &allowed_roles,
                            );
                        return FunctionBody::Expr(body, source_map);
                    }
                }
            }
            // Client has no options block - create empty options and still return PrimitiveClient
            let file_id = file.file_id(db);
            let (body, source_map) = body::empty_primitive_client_body(
                file_id,
                client_name.as_str(),
                &provider,
                &default_role,
                &allowed_roles,
            );
            return FunctionBody::Expr(body, source_map);
        }
    }

    // Compiler-generated LLM functions: synthetic body calling the appropriate builtin.
    if let Some(ref cg) = func.compiler_generated {
        let sig = function_signature(db, function);
        let param_names: Vec<Name> = sig.params.iter().map(|p| p.name.clone()).collect();
        match cg {
            item_tree::CompilerGenerated::LlmCall { base_name } => {
                let (expr_body, source_map) =
                    body::lower_llm_to_call_llm_function(base_name.as_str(), &param_names);
                return FunctionBody::Expr(expr_body, source_map);
            }
            item_tree::CompilerGenerated::LlmRenderPrompt { base_name } => {
                let (expr_body, source_map) =
                    body::lower_llm_to_render_prompt(base_name.as_str(), &param_names);
                return FunctionBody::Expr(expr_body, source_map);
            }
            item_tree::CompilerGenerated::LlmBuildRequest { base_name } => {
                let (expr_body, source_map) =
                    body::lower_llm_to_build_request(base_name.as_str(), &param_names);
                return FunctionBody::Expr(expr_body, source_map);
            }
            item_tree::CompilerGenerated::ClientResolve { .. } => {
                unreachable!("ClientResolve is handled by the early-return block above")
            }
        }
    }

    // Regular function - find it in the source file
    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    let function_def = source_file.items().find_map(|item| match item {
        baml_compiler_syntax::ast::Item::Function(func_node) => {
            // Top-level functions: compare directly
            if func_node.name().as_ref().map(SyntaxToken::text) == Some(&func_name) {
                Some(func_node)
            } else {
                None
            }
        }
        baml_compiler_syntax::ast::Item::Class(class_node) => {
            // Methods: func_name is qualified (ClassName.methodName), so build qualified name
            class_node.methods().find(|method| {
                if let (Some(method_name), Some(class_name_token)) =
                    (method.name(), class_node.name())
                {
                    let qualified_method_name = QualifiedName::local_method_from_str(
                        class_name_token.text(),
                        method_name.text(),
                    );
                    qualified_method_name.as_str() == func_name.as_str()
                } else {
                    false
                }
            })
        }
        _ => None,
    });

    // Lower the function with file_id for span tracking.
    let file_id = file.file_id(db);
    let known_type_names = collect_known_type_names(db);
    function_def.map_or(FunctionBody::Missing, |f| {
        FunctionBody::lower(&f, file_id, known_type_names)
    })
}

/// Returns `true` if this function is one of the expanded LLM pieces (`LlmCall`, `LlmRenderPrompt`, `LlmBuildRequest`).
///
/// This is a cheap check that only reads the `ItemTree`.
pub fn is_llm_function(db: &dyn Db, function: FunctionLoc<'_>) -> bool {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    matches!(
        &func.compiler_generated,
        Some(
            item_tree::CompilerGenerated::LlmCall { .. }
                | item_tree::CompilerGenerated::LlmRenderPrompt { .. }
                | item_tree::CompilerGenerated::LlmBuildRequest { .. }
        )
    )
}

/// Returns the LLM metadata (prompt template + client) for an LLM function.
///
/// Returns `None` for non-LLM functions or malformed LLM functions where the
/// client/prompt can't be extracted from the CST. This is a separate query from
/// `function_body` so that prompt/client changes don't affect the `ItemTree`
/// (preserving early cutoff on body-only changes).
#[salsa::tracked]
pub fn llm_function_meta<'db>(db: &'db dyn Db, function: FunctionLoc<'db>) -> Option<Arc<LlmBody>> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];

    // Only the main-call function (LlmCall) has LLM metadata; render_prompt/build_request do not.
    let base_name = match &func.compiler_generated {
        Some(item_tree::CompilerGenerated::LlmCall { base_name }) => base_name.clone(),
        _ => return None,
    };

    // Go back to the CST to extract prompt template and client
    let func_name = base_name;
    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    source_file.items().find_map(|item| {
        if let baml_compiler_syntax::ast::Item::Function(func_node) = item {
            if func_node.name().as_ref().map(SyntaxToken::text) == Some(&func_name) {
                if let Some(llm_body_node) = func_node.llm_body() {
                    let client = llm_body_node
                        .client_field()
                        .and_then(|cf| cf.value())
                        .map(|v| Name::new(&v));
                    let prompt = llm_body_node
                        .prompt_field()
                        .and_then(|pf| pf.raw_string())
                        .map(|raw_str| body::PromptTemplate::from_raw_string(&raw_str));

                    if let (Some(client), Some(prompt)) = (client, prompt) {
                        return Some(Arc::new(LlmBody { client, prompt }));
                    }
                }
            }
        }
        None
    })
}

//
// ────────────────────────────────────────────────── TEMPLATE STRING QUERIES ─────
//

/// Returns the signature of a template string (parameters).
///
/// This is separate from the `ItemTree` to provide fine-grained incrementality.
#[salsa::tracked]
pub fn template_string_signature<'db>(
    db: &'db dyn Db,
    template_string: TemplateStringLoc<'db>,
) -> Arc<TemplateStringSignature> {
    let (signature, _source_map) = template_string_signature_with_source_map(db, template_string);
    signature
}

/// Returns the source map for a template string signature (parameter spans).
#[salsa::tracked]
pub fn template_string_signature_source_map<'db>(
    db: &'db dyn Db,
    template_string: TemplateStringLoc<'db>,
) -> SignatureSourceMap {
    let (_signature, source_map) = template_string_signature_with_source_map(db, template_string);
    source_map
}

/// Internal helper that computes both signature and source map together.
fn template_string_signature_with_source_map<'db>(
    db: &'db dyn Db,
    template_string: TemplateStringLoc<'db>,
) -> (Arc<TemplateStringSignature>, SignatureSourceMap) {
    let file = template_string.file(db);
    let item_tree = file_item_tree(db, file);
    let ts = &item_tree[template_string.id(db)];
    let ts_name = ts.name.clone();

    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    let default_signature = (
        Arc::new(TemplateStringSignature {
            name: ts.name.clone(),
            params: vec![],
        }),
        SignatureSourceMap::default(),
    );

    let ts_def = source_file.items().find_map(|item| {
        if let baml_compiler_syntax::ast::Item::TemplateString(ts_node) = item {
            if ts_node.name().as_ref().map(SyntaxToken::text) == Some(&ts_name) {
                return Some(TemplateStringSignature::lower(&ts_node));
            }
        }
        None
    });

    ts_def.unwrap_or(default_signature)
}

/// Returns the body (prompt template) of a template string.
#[salsa::tracked]
pub fn template_string_body<'db>(
    db: &'db dyn Db,
    template_string: TemplateStringLoc<'db>,
) -> Arc<PromptTemplate> {
    let file = template_string.file(db);
    let item_tree = file_item_tree(db, file);
    let ts = &item_tree[template_string.id(db)];
    let ts_name = ts.name.clone();

    let tree = syntax_tree(db, file);
    let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();

    let ts_def = source_file.items().find_map(|item| {
        if let baml_compiler_syntax::ast::Item::TemplateString(ts_node) = item {
            if ts_node.name().as_ref().map(SyntaxToken::text) == Some(&ts_name) {
                return Some(ts_node);
            }
        }
        None
    });

    if let Some(ts_node) = ts_def {
        if let Some(raw_string) = ts_node.raw_string() {
            let prompt = PromptTemplate::from_raw_string(&raw_string);
            return Arc::new(prompt);
        }
    }

    Arc::new(PromptTemplate {
        text: String::new(),
        interpolations: vec![],
    })
}

//
// ──────────────────────────────────────────────────────── INTERN HELPERS ─────
//

/// Intern all items from an `ItemTree` and return their IDs.
///
/// Uses name-based `LocalItemIds` for position-independence.
/// Items are returned sorted by their ID value for deterministic ordering.
fn intern_all_items<'db>(db: &'db dyn Db, file: SourceFile, tree: &ItemTree) -> Vec<ItemId<'db>> {
    let mut items = Vec::new();

    // Intern functions - sort by ID for deterministic order
    let mut funcs: Vec<_> = tree.functions.keys().copied().collect();
    funcs.sort_by_key(|id| id.as_u32());
    for local_id in funcs {
        let loc = FunctionLoc::new(db, file, local_id);
        items.push(ItemId::Function(loc));
    }

    // Intern classes
    let mut classes: Vec<_> = tree.classes.keys().copied().collect();
    classes.sort_by_key(|id| id.as_u32());
    for local_id in classes {
        let loc = ClassLoc::new(db, file, local_id);
        items.push(ItemId::Class(loc));
    }

    // Intern enums
    let mut enums: Vec<_> = tree.enums.keys().copied().collect();
    enums.sort_by_key(|id| id.as_u32());
    for local_id in enums {
        let loc = EnumLoc::new(db, file, local_id);
        items.push(ItemId::Enum(loc));
    }

    // Intern type aliases
    let mut aliases: Vec<_> = tree.type_aliases.keys().copied().collect();
    aliases.sort_by_key(|id| id.as_u32());
    for local_id in aliases {
        let loc = TypeAliasLoc::new(db, file, local_id);
        items.push(ItemId::TypeAlias(loc));
    }

    // Intern clients
    let mut clients: Vec<_> = tree.clients.keys().copied().collect();
    clients.sort_by_key(|id| id.as_u32());
    for local_id in clients {
        let loc = ClientLoc::new(db, file, local_id);
        items.push(ItemId::Client(loc));
    }

    // Intern tests
    let mut tests: Vec<_> = tree.tests.keys().copied().collect();
    tests.sort_by_key(|id| id.as_u32());
    for local_id in tests {
        let loc = TestLoc::new(db, file, local_id);
        items.push(ItemId::Test(loc));
    }

    // Intern generators
    let mut generators: Vec<_> = tree.generators.keys().copied().collect();
    generators.sort_by_key(|id| id.as_u32());
    for local_id in generators {
        let loc = GeneratorLoc::new(db, file, local_id);
        items.push(ItemId::Generator(loc));
    }

    // Intern template strings
    let mut template_strings: Vec<_> = tree.template_strings.keys().copied().collect();
    template_strings.sort_by_key(|id| id.as_u32());
    for local_id in template_strings {
        let loc = TemplateStringLoc::new(db, file, local_id);
        items.push(ItemId::TemplateString(loc));
    }

    // Intern retry policies
    let mut retry_policies: Vec<_> = tree.retry_policies.keys().copied().collect();
    retry_policies.sort_by_key(|id| id.as_u32());
    for local_id in retry_policies {
        let loc = RetryPolicyLoc::new(db, file, local_id);
        items.push(ItemId::RetryPolicy(loc));
    }

    items
}

//
// ──────────────────────────────────────────────────────── ITEM LOOKUP ─────
//

// Note: With the Index implementations on ItemTree, you can now use:
//   let item_tree = file_item_tree(db, source_file);
//   let func = &item_tree[func_id.id(db)];
//
// The old lookup helper functions are removed in favor of direct indexing.

//
// ──────────────────────────────────────────────────── CST → HIR LOWERING ─────
//

/// Lower a syntax tree into an `ItemTree` with validation, collecting diagnostics.
///
/// This is the main extraction logic that walks the CST and builds
/// position-independent item representations while validating for errors
/// like duplicate fields, duplicate attributes, etc.
fn lower_file_with_ctx(root: &SyntaxNode, file_id: FileId) -> (ItemTree, Vec<HirDiagnostic>) {
    let mut tree = ItemTree::new();
    let mut ctx = LoweringContext::new(file_id);

    // Walk only direct children of the root (top-level items)
    // Don't use descendants() because that would pick up nested items like
    // CLIENT_DEF nodes inside function bodies
    for child in root.children() {
        lower_item(&mut tree, &child, &mut ctx);
    }

    (tree, ctx.finish())
}

/// Lower a single item from the CST.
fn lower_item(tree: &mut ItemTree, node: &SyntaxNode, ctx: &mut LoweringContext) {
    use baml_compiler_syntax::{SyntaxKind, ast::TypeBuilderBlock};

    match node.kind() {
        SyntaxKind::CLASS_DEF => {
            if let Some(class) = lower_class(node, ctx) {
                tree.alloc_class(class);
            }
            // Desugar methods into top-level functions
            for func in lower_class_methods(node) {
                tree.alloc_function(func);
            }
        }
        SyntaxKind::ENUM_DEF => {
            if let Some(enum_def) = lower_enum(node, ctx) {
                tree.alloc_enum(enum_def);
            }
        }
        SyntaxKind::FUNCTION_DEF => {
            use baml_compiler_syntax::ast::FunctionDef;
            if let Some(func_def) = FunctionDef::cast(node.clone()) {
                if func_def.llm_body().is_some() {
                    // LLM function: expand into Foo, Foo.render_prompt, Foo.build_request
                    if let Some(name_tok) = func_def.name() {
                        let base_name: Name = name_tok.text().into();
                        tree.alloc_function(item_tree::Function {
                            name: base_name.clone(),
                            compiler_generated: Some(item_tree::CompilerGenerated::LlmCall {
                                base_name: base_name.clone(),
                            }),
                        });
                        tree.alloc_function(item_tree::Function {
                            name: Name::new(format!("{base_name}.render_prompt")),
                            compiler_generated: Some(
                                item_tree::CompilerGenerated::LlmRenderPrompt {
                                    base_name: base_name.clone(),
                                },
                            ),
                        });
                        tree.alloc_function(item_tree::Function {
                            name: Name::new(format!("{base_name}.build_request")),
                            compiler_generated: Some(
                                item_tree::CompilerGenerated::LlmBuildRequest { base_name },
                            ),
                        });
                    }
                    // Malformed LLM function with no name; skip expansion (matches lower_function behavior).
                } else if let Some(func) = lower_function(node) {
                    tree.alloc_function(func);
                }
            } else if let Some(func) = lower_function(node) {
                tree.alloc_function(func);
            }
            // Validate: type_builder blocks are not allowed in functions
            for child in node.descendants() {
                if let Some(tb_block) = TypeBuilderBlock::cast(child) {
                    let keyword_range = tb_block
                        .keyword()
                        .map(|kw| kw.text_range())
                        .unwrap_or_else(|| tb_block.syntax().text_range());
                    ctx.push_diagnostic(HirDiagnostic::TypeBuilderInNonTestContext {
                        context: "function",
                        span: ctx.span(keyword_range),
                    });
                }
            }
        }
        SyntaxKind::TYPE_ALIAS_DEF => {
            if let Some(alias) = lower_type_alias(node) {
                tree.alloc_type_alias(alias);
            }
        }
        SyntaxKind::CLIENT_DEF => {
            if let Some(c) = client::lower_client(node, ctx) {
                // Create a compiler-generated resolve function for the client
                // This function evaluates options and returns a PrimitiveClient
                let client_name = c.name.clone();
                let resolve_fn_name = Name::new(format!("{client_name}.resolve"));
                let resolve_fn = item_tree::Function {
                    name: resolve_fn_name,
                    compiler_generated: Some(item_tree::CompilerGenerated::ClientResolve {
                        client_name,
                    }),
                };
                tree.alloc_function(resolve_fn);

                tree.alloc_client(c);
            }
        }
        SyntaxKind::TEST_DEF => {
            if let Some(t) = test::lower_test(node, ctx) {
                tree.alloc_test(t);
            }
        }
        SyntaxKind::GENERATOR_DEF => {
            if let Some(g) = generator::lower_generator(node, ctx) {
                tree.alloc_generator(g);
            }
        }
        SyntaxKind::TEMPLATE_STRING_DEF => {
            if let Some(ts) = lower_template_string(node) {
                tree.alloc_template_string(ts);
            }
        }
        SyntaxKind::RETRY_POLICY_DEF => {
            if let Some(rp) = lower_retry_policy(node) {
                tree.alloc_retry_policy(rp);
            }
        }
        SyntaxKind::LET_STMT => {
            // Top-level let statements require semicolons.
            // The semicolon is a CHILD of the LET_STMT node (parsed inside the statement).
            let has_semicolon = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .any(|token| token.kind() == SyntaxKind::SEMICOLON);

            if !has_semicolon {
                ctx.push_diagnostic(HirDiagnostic::MissingSemicolon {
                    span: ctx.span(node.text_range()),
                });
            }
        }
        _ => {
            // Skip other nodes (whitespace, comments, etc.)
        }
    }
}

//
// ──────────────────────────────────── SAP ATTRIBUTE PARSING ─────
//

/// Which SAP field attribute slot to populate.
#[derive(Clone, Copy)]
enum SapFieldKind {
    CompletedMissing,
    InProgressMissing,
}

/// Parse an @sap.class_*_`field_missing` attribute from synthesized CST into `FieldAttr`.
fn parse_sap_field_attr(
    attr: &baml_compiler_syntax::ast::Attribute,
    existing: &FieldAttr,
    kind: SapFieldKind,
) -> FieldAttr {
    let value = parse_sap_attr_value(attr);
    let inner = existing
        .0
        .as_ref()
        .map(|i| FieldAttrInner {
            sap_class_completed_field_missing: i.sap_class_completed_field_missing.clone(),
            sap_class_in_progress_field_missing: i.sap_class_in_progress_field_missing.clone(),
        })
        .unwrap_or(FieldAttrInner {
            sap_class_completed_field_missing: SapAttrValue::Never,
            sap_class_in_progress_field_missing: SapAttrValue::Never,
        });
    let inner = match kind {
        SapFieldKind::CompletedMissing => FieldAttrInner {
            sap_class_completed_field_missing: value,
            ..inner
        },
        SapFieldKind::InProgressMissing => FieldAttrInner {
            sap_class_in_progress_field_missing: value,
            ..inner
        },
    };
    FieldAttr(Some(Box::new(inner)))
}

/// Parse a @@`sap.in_progress` block attribute from synthesized CST into `TyAttr`.
fn parse_sap_type_attr(attr: &baml_compiler_syntax::ast::BlockAttribute) -> TyAttr {
    let value = parse_sap_block_attr_value(attr);
    TyAttr(Some(Box::new(TyAttrInner {
        sap_in_progress: value,
    })))
}

/// Parse an @sap.* attribute argument into a `SapAttrValue`.
/// Accepts: "never", "null", "[]", "{}", "true", "false", integers, floats, strings.
fn parse_sap_attr_value(attr: &baml_compiler_syntax::ast::Attribute) -> SapAttrValue {
    match attr.string_arg().as_deref() {
        Some("never") => SapAttrValue::Never,
        Some("null") => SapAttrValue::ConstValueExpr(SapConstValue::Null),
        Some("[]") => SapAttrValue::ConstValueExpr(SapConstValue::EmptyList),
        Some("{}") => SapAttrValue::ConstValueExpr(SapConstValue::EmptyMap),
        Some("true") => SapAttrValue::ConstValueExpr(SapConstValue::Bool(true)),
        Some("false") => SapAttrValue::ConstValueExpr(SapConstValue::Bool(false)),
        Some(s) => {
            if let Ok(i) = s.parse::<i64>() {
                SapAttrValue::ConstValueExpr(SapConstValue::Int(i))
            } else if s.parse::<f64>().is_ok() && !s.contains(|c: char| c.is_alphabetic()) {
                SapAttrValue::ConstValueExpr(SapConstValue::Float(s.to_string()))
            } else if let Some((left, right)) = s.split_once('.') {
                // Enum value pattern: Foo.Bar
                SapAttrValue::ConstValueExpr(SapConstValue::EnumValue {
                    enum_name: left.to_string(),
                    variant_name: right.to_string(),
                })
            } else {
                SapAttrValue::ConstValueExpr(SapConstValue::String(s.to_string()))
            }
        }
        None => SapAttrValue::Never,
    }
}

/// Parse a @@sap.* block attribute argument into a `SapAttrValue`.
/// Same logic as `parse_sap_attr_value` but for `BlockAttribute` type.
fn parse_sap_block_attr_value(attr: &baml_compiler_syntax::ast::BlockAttribute) -> SapAttrValue {
    match attr.string_arg().as_deref() {
        Some("never") => SapAttrValue::Never,
        Some("null") => SapAttrValue::ConstValueExpr(SapConstValue::Null),
        Some("[]") => SapAttrValue::ConstValueExpr(SapConstValue::EmptyList),
        Some("{}") => SapAttrValue::ConstValueExpr(SapConstValue::EmptyMap),
        Some("true") => SapAttrValue::ConstValueExpr(SapConstValue::Bool(true)),
        Some("false") => SapAttrValue::ConstValueExpr(SapConstValue::Bool(false)),
        Some(s) => {
            if let Ok(i) = s.parse::<i64>() {
                SapAttrValue::ConstValueExpr(SapConstValue::Int(i))
            } else if s.parse::<f64>().is_ok() && !s.contains(|c: char| c.is_alphabetic()) {
                SapAttrValue::ConstValueExpr(SapConstValue::Float(s.to_string()))
            } else if let Some((left, right)) = s.split_once('.') {
                // Enum value pattern: Foo.Bar
                SapAttrValue::ConstValueExpr(SapConstValue::EnumValue {
                    enum_name: left.to_string(),
                    variant_name: right.to_string(),
                })
            } else {
                SapAttrValue::ConstValueExpr(SapConstValue::String(s.to_string()))
            }
        }
        None => SapAttrValue::Never,
    }
}

/// Extract class definition from CST with validation.
pub(crate) fn lower_class(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Class> {
    use baml_compiler_syntax::ast::ClassDef;

    let class = ClassDef::cast(node.clone())?;
    let name_token = class.name()?;
    let name: Name = name_token.text().into();
    let mut fields = Vec::new();

    // Track seen field names for duplicate detection
    let mut seen_fields: FxHashMap<Name, Span> = FxHashMap::default();

    // Extract fields with duplicate validation
    for field_node in class.fields() {
        if let Some(field_name_token) = field_node.name() {
            let field_name: Name = field_name_token.text().into();
            let field_span = ctx.span(field_name_token.text_range());

            // Check for duplicate field
            if let Some(first_span) = seen_fields.get(&field_name) {
                ctx.push_diagnostic(HirDiagnostic::DuplicateField {
                    class_name: name.to_string(),
                    field_name: field_name.to_string(),
                    first_span: *first_span,
                    second_span: field_span,
                });
            } else {
                seen_fields.insert(field_name.clone(), field_span);
            }

            // Extract field attributes
            let mut field_alias = Attribute::Unset;
            let mut field_description = Attribute::Unset;
            let mut field_skip = Attribute::Unset;
            let mut field_attr = FieldAttr::default();

            // Validate field attributes for duplicates and constraint syntax
            let mut seen_field_attrs: FxHashMap<String, Span> = FxHashMap::default();
            for attr in field_node.attributes() {
                // Use full_name() to get the complete attribute path (e.g., "stream.done" not just "stream")
                if let Some(attr_name) = attr.full_name() {
                    let attr_span =
                        attr.full_name_range()
                            .map(|r| ctx.span(r))
                            .unwrap_or_else(|| {
                                attr.name()
                                    .map(|t| ctx.span(t.text_range()))
                                    .unwrap_or_default()
                            });

                    // check and assert are allowed multiple times on a field
                    if attr_name != "check" && attr_name != "assert" {
                        if let Some(first_span) = seen_field_attrs.get(&attr_name) {
                            ctx.push_diagnostic(HirDiagnostic::DuplicateFieldAttribute {
                                container_kind: "class",
                                container_name: name.to_string(),
                                field_name: field_name.to_string(),
                                attr_name: attr_name.clone(),
                                first_span: *first_span,
                                second_span: attr_span,
                            });
                        } else {
                            seen_field_attrs.insert(attr_name.clone(), attr_span);
                        }
                    }

                    // Extract and validate attribute values
                    match attr_name.as_str() {
                        "alias" => {
                            // @alias requires exactly one string literal argument
                            if attr.has_single_string_arg() {
                                if let Some(value) = attr.string_arg() {
                                    field_alias = Attribute::Explicit(value);
                                }
                            } else {
                                // Invalid: wrong number of args or wrong type
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                    received: describe_attribute_args(&attr),
                                });
                            }
                        }
                        "description" => {
                            // @description accepts quoted or unquoted strings
                            if attr.has_single_string_or_unquoted_arg() {
                                if let Some(value) = attr.string_arg() {
                                    field_description = Attribute::Explicit(value);
                                }
                            } else {
                                // Invalid: wrong number of args or wrong type
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                    received: describe_attribute_args(&attr),
                                });
                            }
                        }
                        "skip" => {
                            // @skip takes no arguments
                            if attr.has_args() {
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::UnexpectedAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                });
                            }
                            field_skip = Attribute::Explicit(());
                        }
                        "check" | "assert" => {
                            // Validate constraint attribute syntax
                            validate_constraint_attribute(&attr, &attr_name, attr_span, ctx);
                        }
                        // SAP field attributes (from synthesized stream_* nodes)
                        "sap.class_completed_field_missing" => {
                            field_attr = parse_sap_field_attr(
                                &attr,
                                &field_attr,
                                SapFieldKind::CompletedMissing,
                            );
                        }
                        "sap.class_in_progress_field_missing" => {
                            field_attr = parse_sap_field_attr(
                                &attr,
                                &field_attr,
                                SapFieldKind::InProgressMissing,
                            );
                        }
                        // @stream.* attributes are consumed by PPIR, silently skip
                        a if a.starts_with("stream.") => {}
                        _ => {
                            // Other attributes - just validate duplicates
                        }
                    }
                }
            }

            // Validate map type arity before converting to TypeRef
            if let Some(type_expr) = field_node.ty() {
                validate_map_type_arity(&type_expr, ctx);
            }

            // Extract TypeRef and check for type-level SAP attributes
            let type_ref = field_node
                .ty()
                .map(|t| TypeRef::from_ast(&t))
                .unwrap_or_else(TypeRef::unknown);

            fields.push(crate::Field {
                name: field_name,
                type_ref,
                alias: field_alias,
                description: field_description,
                skip: field_skip,
                field_attr,
            });
        }
    }

    // Track seen block attributes for duplicate detection
    let mut seen_attrs: FxHashMap<String, Span> = FxHashMap::default();
    let mut class_is_dynamic = Attribute::Unset;
    let mut class_alias = Attribute::Unset;
    let mut class_description = Attribute::Unset;
    let mut class_ty_attr = TyAttr::default();

    // Validate block attributes
    for attr in class.block_attributes() {
        // Use full_name() to get the complete attribute path (e.g., "stream.done" not just "stream")
        if let Some(attr_name) = attr.full_name() {
            // Use the full attribute name range for precise error highlighting
            let attr_span = attr
                .full_name_range()
                .map(|r| ctx.span(r))
                .unwrap_or_else(|| {
                    attr.name()
                        .map(|t| ctx.span(t.text_range()))
                        .unwrap_or_default()
                });

            // Check for duplicate attribute
            if let Some(first_span) = seen_attrs.get(&attr_name) {
                ctx.push_diagnostic(HirDiagnostic::DuplicateBlockAttribute {
                    item_kind: "class",
                    item_name: name.to_string(),
                    attr_name: attr_name.clone(),
                    first_span: *first_span,
                    second_span: attr_span,
                });
            } else {
                seen_attrs.insert(attr_name.clone(), attr_span);
            }

            // Extract and validate attribute values
            match attr_name.as_str() {
                "dynamic" => {
                    // @@dynamic takes no arguments
                    if attr.has_args() {
                        let arg_span = attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                        ctx.push_diagnostic(HirDiagnostic::UnexpectedAttributeArg {
                            attr_name: attr_name.clone(),
                            span: arg_span,
                        });
                    }
                    class_is_dynamic = Attribute::Explicit(());
                }
                "alias" => {
                    // @@alias requires exactly one string literal argument
                    if attr.has_single_string_arg() {
                        if let Some(value) = attr.string_arg() {
                            class_alias = Attribute::Explicit(value);
                        }
                    } else {
                        // Invalid: wrong number of args or wrong type
                        let arg_span = attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                        ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                            attr_name: attr_name.clone(),
                            span: arg_span,
                            received: describe_block_attribute_args(&attr),
                        });
                    }
                }
                "description" => {
                    // @@description accepts quoted or unquoted strings
                    if attr.has_single_string_or_unquoted_arg() {
                        if let Some(value) = attr.string_arg() {
                            class_description = Attribute::Explicit(value);
                        }
                    } else {
                        // Invalid: wrong number of args or wrong type
                        let arg_span = attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                        ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                            attr_name: attr_name.clone(),
                            span: arg_span,
                            received: describe_block_attribute_args(&attr),
                        });
                    }
                }
                // SAP block attribute (from synthesized stream_* nodes)
                "sap.in_progress" => {
                    class_ty_attr = parse_sap_type_attr(&attr);
                }
                // @@stream.* attributes are consumed by PPIR, silently skip
                a if a.starts_with("stream.") => {}
                _ => {
                    // Other attributes - just validate duplicates
                }
            }
        }
    }

    Some(Class {
        name,
        fields,
        is_dynamic: class_is_dynamic,
        alias: class_alias,
        description: class_description,
        ty_attr: class_ty_attr,
    })
}

/// Extract desugared method functions from a class.
/// Methods like `class Baz { function Greeting(self) }` become top-level functions `Baz.Greeting(self: Baz)`.
/// The method name is qualified with the class name to ensure uniqueness and match TIR resolution.
fn lower_class_methods(node: &SyntaxNode) -> Vec<Function> {
    use baml_compiler_syntax::ast::ClassDef;

    let Some(class) = ClassDef::cast(node.clone()) else {
        return Vec::new();
    };

    let class_name = class
        .name()
        .map(|t| t.text().to_string())
        .unwrap_or_else(|| "UnnamedClass".to_string());

    let mut functions = Vec::new();
    for method_node in class.methods() {
        if let Some(method_name) = method_node.name() {
            // Use qualified name: ClassName.methodName
            // This ensures methods are uniquely identified and matches how they're
            // resolved in TIR (via QualifiedName::local_method)
            let qualified_name =
                QualifiedName::local_method_from_str(&class_name, method_name.text());
            functions.push(Function {
                name: qualified_name,
                compiler_generated: None,
            });
        }
    }
    functions
}

/// Extract enum definition from CST with validation.
pub(crate) fn lower_enum(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Enum> {
    use baml_compiler_syntax::ast::EnumDef;

    let enum_def = EnumDef::cast(node.clone())?;

    // Check if the enum has proper structure (braces)
    // Malformed enums from error recovery (e.g., "enum" without name/braces) should be skipped
    if !enum_def.has_body() {
        return None;
    }

    // Extract name using AST accessor
    let name = enum_def
        .name()
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("UnnamedEnum"));

    // Track seen variant names for duplicate detection
    let mut seen_variants: FxHashMap<Name, Span> = FxHashMap::default();
    let mut variants = Vec::new();

    // Extract variants with duplicate validation
    for variant in enum_def.variants() {
        if let Some(name_token) = variant.name() {
            let variant_name = Name::new(name_token.text());
            let variant_span = ctx.span(name_token.text_range());

            // Check for duplicate variant
            if let Some(first_span) = seen_variants.get(&variant_name) {
                ctx.push_diagnostic(HirDiagnostic::DuplicateVariant {
                    enum_name: name.to_string(),
                    variant_name: variant_name.to_string(),
                    first_span: *first_span,
                    second_span: variant_span,
                });
            } else {
                seen_variants.insert(variant_name.clone(), variant_span);
            }

            // Extract variant attributes
            let mut variant_alias = Attribute::Unset;
            let mut variant_description = Attribute::Unset;
            let mut variant_skip = Attribute::Unset;

            // Validate variant attributes for duplicates
            let mut seen_variant_attrs: FxHashMap<String, Span> = FxHashMap::default();
            for attr in variant.attributes() {
                // Use full_name() to get the complete attribute path (e.g., "stream.done" not just "stream")
                if let Some(attr_name) = attr.full_name() {
                    let attr_span =
                        attr.full_name_range()
                            .map(|r| ctx.span(r))
                            .unwrap_or_else(|| {
                                attr.name()
                                    .map(|t| ctx.span(t.text_range()))
                                    .unwrap_or_default()
                            });

                    // check and assert are allowed multiple times on a variant
                    if attr_name != "check" && attr_name != "assert" {
                        if let Some(first_span) = seen_variant_attrs.get(&attr_name) {
                            ctx.push_diagnostic(HirDiagnostic::DuplicateFieldAttribute {
                                container_kind: "enum",
                                container_name: name.to_string(),
                                field_name: variant_name.to_string(),
                                attr_name: attr_name.clone(),
                                first_span: *first_span,
                                second_span: attr_span,
                            });
                        } else {
                            seen_variant_attrs.insert(attr_name.clone(), attr_span);
                        }
                    }

                    // Extract and validate attribute values
                    match attr_name.as_str() {
                        "alias" => {
                            // @alias requires exactly one string literal argument
                            if attr.has_single_string_arg() {
                                if let Some(value) = attr.string_arg() {
                                    variant_alias = Attribute::Explicit(value);
                                }
                            } else {
                                // Invalid: wrong number of args or wrong type
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                    received: describe_attribute_args(&attr),
                                });
                            }
                        }
                        "description" => {
                            // @description accepts quoted or unquoted strings
                            if attr.has_single_string_or_unquoted_arg() {
                                if let Some(value) = attr.string_arg() {
                                    variant_description = Attribute::Explicit(value);
                                }
                            } else {
                                // Invalid: wrong number of args or wrong type
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                    received: describe_attribute_args(&attr),
                                });
                            }
                        }
                        "skip" => {
                            // @skip takes no arguments
                            if attr.has_args() {
                                let arg_span =
                                    attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                                ctx.push_diagnostic(HirDiagnostic::UnexpectedAttributeArg {
                                    attr_name: attr_name.clone(),
                                    span: arg_span,
                                });
                            }
                            variant_skip = Attribute::Explicit(());
                        }
                        _ => {
                            // Other attributes - just validate duplicates
                        }
                    }
                }
            }

            variants.push(crate::EnumVariant {
                name: variant_name,
                alias: variant_alias,
                description: variant_description,
                skip: variant_skip,
            });
        }
    }

    // Track seen block attributes for duplicate detection
    let mut seen_attrs: FxHashMap<String, Span> = FxHashMap::default();
    let mut enum_alias = Attribute::Unset;

    // Validate block attributes
    for attr in enum_def.block_attributes() {
        // Use full_name() to get the complete attribute path (e.g., "stream.done" not just "stream")
        if let Some(attr_name) = attr.full_name() {
            // Use the full attribute name range for precise error highlighting
            let attr_span = attr
                .full_name_range()
                .map(|r| ctx.span(r))
                .unwrap_or_else(|| {
                    attr.name()
                        .map(|t| ctx.span(t.text_range()))
                        .unwrap_or_default()
                });

            // Check for duplicate attribute
            if let Some(first_span) = seen_attrs.get(&attr_name) {
                ctx.push_diagnostic(HirDiagnostic::DuplicateBlockAttribute {
                    item_kind: "enum",
                    item_name: name.to_string(),
                    attr_name: attr_name.clone(),
                    first_span: *first_span,
                    second_span: attr_span,
                });
            } else {
                seen_attrs.insert(attr_name.clone(), attr_span);
            }

            // Extract and validate attribute values
            match attr_name.as_str() {
                "alias" => {
                    // @@alias requires exactly one string literal argument
                    if attr.has_single_string_arg() {
                        if let Some(value) = attr.string_arg() {
                            enum_alias = Attribute::Explicit(value);
                        }
                    } else {
                        // Invalid: wrong number of args or wrong type
                        let arg_span = attr.args_span().map(|r| ctx.span(r)).unwrap_or(attr_span);
                        ctx.push_diagnostic(HirDiagnostic::InvalidAttributeArg {
                            attr_name: attr_name.clone(),
                            span: arg_span,
                            received: describe_block_attribute_args(&attr),
                        });
                    }
                }
                _ => {
                    // Other attributes - just validate duplicates
                }
            }
        }
    }

    Some(Enum {
        name,
        variants,
        alias: enum_alias,
        ty_attr: TyAttr::default(),
    })
}

/// Extract function definition from CST - MINIMAL VERSION.
/// Only extracts the name. Signature and body are in separate queries.
/// LLM functions are not handled here; they are expanded into three functions in the `FUNCTION_DEF` branch.
fn lower_function(node: &SyntaxNode) -> Option<Function> {
    use baml_compiler_syntax::ast::FunctionDef;

    let func = FunctionDef::cast(node.clone())?;
    let name = func.name()?.text().into();

    Some(Function {
        name,
        compiler_generated: None,
    })
}

/// Extract template string from CST.
fn lower_template_string(node: &SyntaxNode) -> Option<item_tree::TemplateString> {
    use baml_compiler_syntax::ast::TemplateStringDef;

    let ts = TemplateStringDef::cast(node.clone())?;
    let name = ts.name()?.text().into();

    Some(item_tree::TemplateString { name })
}

/// Extract retry policy from CST.
fn lower_retry_policy(node: &SyntaxNode) -> Option<item_tree::RetryPolicy> {
    use baml_compiler_syntax::ast::RetryPolicyDef;

    let rp = RetryPolicyDef::cast(node.clone())?;
    let name = rp.name()?.text().into();

    let mut max_retries = None;
    let mut initial_delay_ms = None;
    let mut multiplier = None;
    let mut max_delay_ms = None;

    if let Some(config_block) = rp.config_block() {
        // Extract max_retries from the top-level config block
        if let Some(item) = config_block.items().find(|i| i.matches_key("max_retries")) {
            max_retries = item.value_int().map(|v| v.to_string());
        }

        // Extract delay/multiplier/max_delay fields from either:
        // 1. A `strategy` sub-block (traditional syntax):
        //      strategy { type exponential_backoff  delay_ms 100  multiplier 2 }
        // 2. Top-level config keys (flat syntax):
        //      initial_delay_ms 100  multiplier 2  max_delay_ms 1000
        if let Some(strategy_item) = config_block.items().find(|i| i.matches_key("strategy")) {
            if let Some(strategy_block) = strategy_item.nested_block() {
                for item in strategy_block.items() {
                    let Some(key) = item.key() else { continue };
                    match key.text() {
                        "delay_ms" => {
                            initial_delay_ms = item.value_int().map(|v| v.to_string());
                        }
                        "multiplier" => {
                            // multiplier can be a float, so use value_str
                            multiplier = item.value_str();
                        }
                        "max_delay_ms" => {
                            max_delay_ms = item.value_int().map(|v| v.to_string());
                        }
                        _ => {
                            // Ignore unknown fields like "type" for now
                        }
                    }
                }
            }
        } else {
            // Flat syntax: delay fields at top level
            for item in config_block.items() {
                let Some(key) = item.key() else { continue };
                match key.text() {
                    "initial_delay_ms" | "delay_ms" => {
                        initial_delay_ms = item.value_int().map(|v| v.to_string());
                    }
                    "multiplier" => {
                        multiplier = item.value_str();
                    }
                    "max_delay_ms" => {
                        max_delay_ms = item.value_int().map(|v| v.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    Some(item_tree::RetryPolicy {
        name,
        max_retries,
        initial_delay_ms,
        multiplier,
        max_delay_ms,
    })
}

/// Extract type alias from CST.
pub(crate) fn lower_type_alias(node: &SyntaxNode) -> Option<TypeAlias> {
    use baml_compiler_syntax::ast::TypeAliasDef;

    let alias = TypeAliasDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name = alias
        .name()
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("UnnamedTypeAlias"));

    // Extract type using AST accessor
    let type_ref = alias
        .ty()
        .map(|t| TypeRef::from_ast(&t))
        .unwrap_or_else(TypeRef::unknown);

    Some(TypeAlias { name, type_ref })
}

//
// ────────────────────────────────────────────────────── NAME VALIDATION ─────
//

use rustc_hash::FxHashMap;

/// Information about a named item for duplicate detection.
struct ItemInfo {
    span: Span,
    path: String,
    kind: &'static str,
    /// Whether we've already emitted an error for this item as the "first definition".
    first_error_emitted: bool,
}

/// Result of HIR validation.
pub struct HirValidationResult {
    /// HIR-level diagnostics (field duplicates, reserved names, etc.).
    pub hir_diagnostics: Vec<HirDiagnostic>,
    /// Name errors (duplicate top-level names, etc.).
    pub name_errors: Vec<NameError>,
}

/// Run all HIR-level validations on a project.
///
/// This is the main entry point for HIR validation. It runs:
/// - Duplicate name detection (classes, functions, etc.)
/// - Reserved name validation (field names that are keywords in target languages)
/// - Field name matches type name validation (Python-specific)
pub fn validate_hir(db: &dyn Db, root: baml_workspace::Project) -> HirValidationResult {
    let mut hir_diagnostics = validate_reserved_names(db, root);
    hir_diagnostics.extend(validate_retry_policy_refs(db, root));
    hir_diagnostics.extend(validate_stream_prefix(db, root));
    let mut name_errors = validate_duplicate_names(db, root);
    name_errors.extend(validate_test_functions(db, root));

    HirValidationResult {
        hir_diagnostics,
        name_errors,
    }
}

/// Validate that retry policy references in clients point to existing policies.
fn validate_retry_policy_refs(db: &dyn Db, root: baml_workspace::Project) -> Vec<HirDiagnostic> {
    // Collect all retry policy names across all files.
    let mut known_policies: rustc_hash::FxHashSet<Name> = rustc_hash::FxHashSet::default();
    for file in root.files(db) {
        let items_struct = file_items(db, *file);
        for item in items_struct.items(db) {
            if let ItemId::RetryPolicy(rp_loc) = item {
                let item_tree = file_item_tree(db, rp_loc.file(db));
                let rp = &item_tree[rp_loc.id(db)];
                known_policies.insert(rp.name.clone());
            }
        }
    }

    // Check each client's retry_policy_name against the known set.
    let mut errors = Vec::new();
    for file in root.files(db) {
        let items_struct = file_items(db, *file);
        let file_id = file.file_id(db);
        for item in items_struct.items(db) {
            if let ItemId::Client(client_loc) = item {
                let item_tree = file_item_tree(db, client_loc.file(db));
                let client = &item_tree[client_loc.id(db)];
                if let Some(ref policy_name) = client.retry_policy_name {
                    if !known_policies.contains(policy_name) {
                        let span = client
                            .retry_policy_span
                            .map(|range| Span::new(file_id, range))
                            .unwrap_or_else(|| Span::new(file_id, TextRange::empty(0.into())));
                        errors.push(HirDiagnostic::UnknownRetryPolicy {
                            client_name: client.name.to_string(),
                            policy_name: policy_name.to_string(),
                            span,
                        });
                    }
                }
            }
        }
    }
    errors
}

fn validate_test_functions(db: &dyn Db, root: baml_workspace::Project) -> Vec<NameError> {
    let symbol_table = symbol_table::symbol_table(db, root);
    let mut errors = Vec::new();

    for file in root.files(db) {
        let tree = syntax_tree(db, *file);
        let source_file = baml_compiler_syntax::ast::SourceFile::cast(tree).unwrap();
        let file_id = file.file_id(db);

        for item in source_file.items() {
            if let baml_compiler_syntax::ast::Item::Test(test_def) = item {
                for func_token in test_def.function_names() {
                    let name = Name::new(func_token.text());
                    let fqn = QualifiedName::local(name.clone());
                    if symbol_table.lookup_value(db, &fqn).is_none() {
                        errors.push(NameError::UnknownFunctionInTest {
                            function_name: name.to_string(),
                            span: Span::new(file_id, func_token.text_range()),
                        });
                    }
                }
            }
        }
    }
    errors
}

/// Validate that there are no duplicate names in the project.
///
/// Top-level entities (classes, enums, functions, type aliases, clients)
/// share the same namespace, so any duplicate name is an error.
///
/// Tests are validated separately: only tests with the same name AND
/// targeting the same function are considered duplicates.
fn validate_duplicate_names(db: &dyn Db, root: baml_workspace::Project) -> Vec<NameError> {
    fn item_name_key(db: &dyn Db, file: SourceFile, name: &Name) -> Name {
        let namespace = file_namespace(db, file).unwrap_or(Namespace::Local);
        QualifiedName {
            namespace,
            name: name.clone(),
        }
        .display_name()
    }

    let items = project_items(db, root);
    let mut seen: FxHashMap<Name, ItemInfo> = FxHashMap::default();
    // For tests: key is (test_name, function_name)
    let mut seen_tests: FxHashMap<(Name, Name), ItemInfo> = FxHashMap::default();
    let mut errors = Vec::new();

    for item in items.items(db) {
        match item {
            ItemId::Function(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let func = &item_tree[local_id];
                let name_key = item_name_key(db, file, &func.name);
                let span =
                    get_item_name_span(db, file, "function", func.name.as_str(), local_id.index())
                        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "function", span, path);
            }
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let class = &item_tree[local_id];
                let name_key = item_name_key(db, file, &class.name);
                let span =
                    get_item_name_span(db, file, "class", class.name.as_str(), local_id.index())
                        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "class", span, path);
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let enum_def = &item_tree[local_id];
                let name_key = item_name_key(db, file, &enum_def.name);
                let span =
                    get_item_name_span(db, file, "enum", enum_def.name.as_str(), local_id.index())
                        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "enum", span, path);
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let alias = &item_tree[local_id];
                let name_key = item_name_key(db, file, &alias.name);
                let span = get_item_name_span(
                    db,
                    file,
                    "type alias",
                    alias.name.as_str(),
                    local_id.index(),
                )
                .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "type alias", span, path);
            }
            ItemId::Client(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let client = &item_tree[local_id];
                let name_key = item_name_key(db, file, &client.name);
                let span =
                    get_item_name_span(db, file, "client", client.name.as_str(), local_id.index())
                        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "client", span, path);
            }
            ItemId::Generator(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let generator = &item_tree[local_id];
                let name_key = item_name_key(db, file, &generator.name);
                let span = get_item_name_span(
                    db,
                    file,
                    "generator",
                    generator.name.as_str(),
                    local_id.index(),
                )
                .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "generator", span, path);
            }
            ItemId::Test(loc) => {
                // Tests are validated separately: only same name + same function is a duplicate
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let test = &item_tree[local_id];
                let span =
                    get_item_name_span(db, file, "test", test.name.as_str(), local_id.index())
                        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();

                // Check each function reference in the test
                for func_ref in &test.function_refs {
                    let key = (test.name.clone(), func_ref.clone());
                    if let Some(existing) = seen_tests.get(&key) {
                        errors.push(NameError::DuplicateTestForFunction {
                            test_name: test.name.to_string(),
                            function_name: func_ref.to_string(),
                            first: existing.span,
                            first_path: existing.path.clone(),
                            second: span,
                            second_path: path.clone(),
                        });
                    } else {
                        seen_tests.insert(
                            key,
                            ItemInfo {
                                span,
                                path: path.clone(),
                                kind: "test",
                                first_error_emitted: false,
                            },
                        );
                    }
                }
            }
            ItemId::TemplateString(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let ts = &item_tree[local_id];
                let span = get_item_name_span(
                    db,
                    file,
                    "template_string",
                    ts.name.as_str(),
                    local_id.index(),
                )
                .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    ts.name.clone(),
                    "template_string",
                    span,
                    path,
                );
            }
            ItemId::RetryPolicy(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let local_id = loc.id(db);
                let rp = &item_tree[local_id];
                let name_key = item_name_key(db, file, &rp.name);
                let span = get_item_name_span(
                    db,
                    file,
                    "retry_policy",
                    rp.name.as_str(),
                    local_id.index(),
                )
                .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let path = file.path(db).display().to_string();
                check_duplicate(&mut seen, &mut errors, name_key, "retry_policy", span, path);
            }
        }
    }

    errors
}

/// Validate that a @check or @assert attribute has proper Jinja expression syntax.
/// These attributes require at least one argument that is a Jinja expression block {{ }}.
fn validate_constraint_attribute(
    attr: &baml_compiler_syntax::Attribute,
    attr_name: &str,
    span: Span,
    ctx: &mut LoweringContext,
) {
    use baml_compiler_syntax::SyntaxKind;

    // Find the ATTRIBUTE_ARGS node
    let args_node = attr
        .syntax()
        .children()
        .find(|n| n.kind() == SyntaxKind::ATTRIBUTE_ARGS);

    if let Some(args) = args_node {
        // Check if any argument is an EXPR node (Jinja expression {{ }})
        let has_expr = args.children().any(|n| n.kind() == SyntaxKind::EXPR);

        if !has_expr {
            ctx.push_diagnostic(HirDiagnostic::InvalidConstraintSyntax {
                attr_name: attr_name.to_string(),
                span,
            });
        }
    } else {
        // No arguments at all - also invalid
        ctx.push_diagnostic(HirDiagnostic::InvalidConstraintSyntax {
            attr_name: attr_name.to_string(),
            span,
        });
    }
}

/// Describe what was received in an attribute's arguments.
///
/// Used to produce error messages like "Expected @alias("..."), but got ..."
fn describe_attribute_args(attr: &baml_compiler_syntax::Attribute) -> String {
    use baml_compiler_syntax::SyntaxKind;

    let arg_count = attr.arg_count();

    match arg_count {
        0 => "no arguments".to_string(),
        1 => {
            // Single argument - describe its type
            if let Some(arg) = attr.args().next() {
                match arg.kind() {
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                        // This shouldn't happen if we're calling this function,
                        // but handle it gracefully
                        format!("`{}`", arg.text())
                    }
                    SyntaxKind::EXPR => {
                        let text = arg.text().to_string();
                        format!("an expression `{text}`")
                    }
                    SyntaxKind::UNQUOTED_STRING => {
                        let text = arg.text().to_string();
                        format!("an expression `{text}`")
                    }
                    _ => "an unknown value".to_string(),
                }
            } else {
                "an unknown value".to_string()
            }
        }
        n => format!("{n} arguments"),
    }
}

/// Describe what was received in a block attribute's arguments.
fn describe_block_attribute_args(attr: &baml_compiler_syntax::ast::BlockAttribute) -> String {
    use baml_compiler_syntax::SyntaxKind;

    let arg_count = attr.arg_count();

    match arg_count {
        0 => "no arguments".to_string(),
        1 => {
            // Single argument - describe its type
            if let Some(arg) = attr.args().next() {
                match arg.kind() {
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                        format!("`{}`", arg.text())
                    }
                    SyntaxKind::EXPR => {
                        let text = arg.text().to_string();
                        format!("an expression `{text}`")
                    }
                    SyntaxKind::UNQUOTED_STRING => {
                        let text = arg.text().to_string();
                        format!("an expression `{text}`")
                    }
                    _ => "an unknown value".to_string(),
                }
            } else {
                "an unknown value".to_string()
            }
        }
        n => format!("{n} arguments"),
    }
}

/// Check if a `TYPE_EXPR` has actual content (not empty from parser error recovery).
///
/// The parser creates empty `TYPE_EXPR` nodes for error recovery (e.g., `map<>`).
/// This function checks if the `TYPE_EXPR` has meaningful content like:
/// - A base name (int, string, User, etc.)
/// - A string literal ("admin")
/// - An integer literal (200)
/// - A bool literal (true/false)
/// - An inner parenthesized type
/// - Is a union type
fn has_type_content(type_expr: &baml_compiler_syntax::ast::TypeExpr) -> bool {
    // Check for base name (primitives and named types)
    if type_expr.base_name().is_some() {
        return true;
    }
    // Check for string literal
    if type_expr.string_literal().is_some() {
        return true;
    }
    // Check for integer literal
    if type_expr.integer_literal().is_some() {
        return true;
    }
    // Check for bool literal
    if type_expr.bool_literal().is_some() {
        return true;
    }
    // Check for parenthesized type (has inner content)
    if type_expr.inner_type_expr().is_some() {
        return true;
    }
    // Check for union type
    if type_expr.is_union() {
        return true;
    }
    false
}

/// Validate map type arity in a type expression.
///
/// Maps require exactly 2 type parameters: `map<K, V>`.
/// This function checks for cases like `map<string, string, string>` (3 params).
fn validate_map_type_arity(
    type_expr: &baml_compiler_syntax::ast::TypeExpr,
    ctx: &mut LoweringContext,
) {
    // For union types, check each member
    if type_expr.is_union() {
        for part in type_expr.union_member_parts() {
            validate_map_type_in_union_member(&part, type_expr, ctx);
        }
    } else {
        // For non-union types, check the type expression directly
        validate_map_type_in_type_expr(type_expr, ctx);
    }
}

/// Validate map types in a union member (token-based).
fn validate_map_type_in_union_member(
    part: &baml_compiler_syntax::ast::UnionMemberParts,
    type_expr: &baml_compiler_syntax::ast::TypeExpr,
    ctx: &mut LoweringContext,
) {
    use rowan::ast::AstNode;

    // Check if this is a map type by looking at first word
    if let Some(name) = part.first_word() {
        if name == "map" {
            // Get TYPE_ARGS and count type parameters
            // Note: Only count TYPE_EXPR nodes that have actual content (base_name).
            // The parser creates empty TYPE_EXPR nodes for error recovery (e.g., map<>).
            if let Some(type_args) = part.type_args() {
                let param_count = type_args
                    .children()
                    .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                    .filter(|n| {
                        // Check if TYPE_EXPR has actual content (not empty from error recovery)
                        // Empty TYPE_EXPR nodes have no meaningful tokens/children
                        baml_compiler_syntax::ast::TypeExpr::cast(n.clone())
                            .map(|te| has_type_content(&te))
                            .unwrap_or(false)
                    })
                    .count();

                if param_count != 2 {
                    ctx.push_diagnostic(HirDiagnostic::InvalidMapArity {
                        expected: 2,
                        found: param_count,
                        span: ctx.span(type_expr.syntax().text_range()),
                    });
                } else {
                    // Recursively check nested types in both key and value
                    for child in type_args.children() {
                        if child.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR {
                            if let Some(child_expr) =
                                baml_compiler_syntax::ast::TypeExpr::cast(child)
                            {
                                validate_map_type_in_type_expr(&child_expr, ctx);
                            }
                        }
                    }
                }
            }
        }
    }

    // Check for nested types in parenthesized expressions
    if let Some(inner_expr) = part.type_expr() {
        validate_map_type_in_type_expr(&inner_expr, ctx);
    }
}

/// Validate map types in a type expression recursively.
fn validate_map_type_in_type_expr(
    type_expr: &baml_compiler_syntax::ast::TypeExpr,
    ctx: &mut LoweringContext,
) {
    // Handle union types
    if type_expr.is_union() {
        for part in type_expr.union_member_parts() {
            validate_map_type_in_union_member(&part, type_expr, ctx);
        }
        return;
    }

    // Handle parenthesized types
    if let Some(inner) = type_expr.inner_type_expr() {
        validate_map_type_in_type_expr(&inner, ctx);
        return;
    }

    // Check if this is a map type
    if let Some(name) = type_expr.base_name() {
        if name == "map" {
            // Filter out empty TYPE_EXPR nodes created by parser error recovery (e.g., map<>)
            let type_args: Vec<_> = type_expr
                .type_arg_exprs()
                .into_iter()
                .filter(has_type_content)
                .collect();
            let param_count = type_args.len();

            if param_count != 2 {
                ctx.push_diagnostic(HirDiagnostic::InvalidMapArity {
                    expected: 2,
                    found: param_count,
                    span: ctx.span(type_expr.syntax().text_range()),
                });
            } else {
                // Recursively check nested types in both key and value
                for arg in type_args {
                    validate_map_type_in_type_expr(&arg, ctx);
                }
            }
        }
    }
}

/// Helper to check for duplicate names and record errors.
fn check_duplicate(
    seen: &mut FxHashMap<Name, ItemInfo>,
    errors: &mut Vec<NameError>,
    name: Name,
    kind: &'static str,
    span: Span,
    path: String,
) {
    if let Some(existing) = seen.get_mut(&name) {
        // If this is the first duplicate we've seen, also emit an error for the first definition
        if !existing.first_error_emitted {
            errors.push(NameError::DuplicateName {
                name: name.to_string(),
                kind: existing.kind,
                first: span,
                first_path: path.clone(),
                second: existing.span,
                second_path: existing.path.clone(),
            });
            existing.first_error_emitted = true;
        }

        // Emit error for the current (duplicate) definition
        errors.push(NameError::DuplicateName {
            name: name.to_string(),
            kind,
            first: existing.span,
            first_path: existing.path.clone(),
            second: span,
            second_path: path,
        });
    } else {
        seen.insert(
            name,
            ItemInfo {
                span,
                path,
                kind,
                first_error_emitted: false,
            },
        );
    }
}

/// Extract the base type name from a `TypeRef`, unwrapping Optional, List, etc.
fn get_base_type_name(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::Path(path, _) => path.last_segment().map(std::string::ToString::to_string),
        TypeRef::Optional(inner, _) => get_base_type_name(inner),
        TypeRef::List(inner, _) => get_base_type_name(inner),
        TypeRef::Generic { base, .. } => get_base_type_name(base),
        _ => None,
    }
}

/// Information about a class field or enum variant from the syntax tree.
struct FieldInfo {
    span: Span,
    has_alias: bool,
}

/// Look up the span and attributes of a field in a class from the syntax tree.
fn get_class_field_info(
    db: &dyn Db,
    file: baml_base::files::SourceFile,
    class_name: &str,
    field_name: &str,
) -> Option<FieldInfo> {
    use baml_compiler_syntax::{SyntaxKind, ast::ClassDef};

    let tree = baml_compiler_parser::syntax_tree(db, file);

    // Find the class node
    for node in tree.children() {
        if node.kind() == SyntaxKind::CLASS_DEF {
            if let Some(class) = ClassDef::cast(node) {
                if let Some(name_token) = class.name() {
                    if name_token.text() == class_name {
                        // Found the class, now find the field
                        for field_node in class.fields() {
                            if let Some(field_name_token) = field_node.name() {
                                if field_name_token.text() == field_name {
                                    // Check if field has @alias attribute
                                    let has_alias = field_node.attributes().any(|attr| {
                                        attr.name().map(|n| n.text() == "alias").unwrap_or(false)
                                    });

                                    return Some(FieldInfo {
                                        span: Span::new(
                                            file.file_id(db),
                                            field_name_token.text_range(),
                                        ),
                                        has_alias,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Look up the span and attributes of a variant in an enum from the syntax tree.
fn get_enum_variant_info(
    db: &dyn Db,
    file: baml_base::files::SourceFile,
    enum_name: &str,
    variant_name: &str,
) -> Option<FieldInfo> {
    use baml_compiler_syntax::{SyntaxKind, ast::EnumDef};

    let tree = baml_compiler_parser::syntax_tree(db, file);

    // Find the enum node
    for node in tree.children() {
        if node.kind() == SyntaxKind::ENUM_DEF {
            if let Some(enum_def) = EnumDef::cast(node) {
                if let Some(name_token) = enum_def.name() {
                    if name_token.text() == enum_name {
                        // Found the enum, now find the variant
                        for variant_node in enum_def.variants() {
                            if let Some(variant_name_token) = variant_node.name() {
                                if variant_name_token.text() == variant_name {
                                    // Check if variant has @alias attribute
                                    let has_alias = variant_node.attributes().any(|attr| {
                                        attr.name().map(|n| n.text() == "alias").unwrap_or(false)
                                    });

                                    return Some(FieldInfo {
                                        span: Span::new(
                                            file.file_id(db),
                                            variant_name_token.text_range(),
                                        ),
                                        has_alias,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Look up the span of a top-level item's name from the syntax tree.
///
/// This is used to get accurate spans for duplicate name errors, since the
/// `ItemTree` is position-independent and doesn't store spans.
///
/// The `occurrence` parameter specifies which occurrence to return (0 = first, 1 = second, etc.)
/// when there are multiple items of the same kind with the same name in the file.
/// This corresponds to the collision index in `LocalItemId`.
pub fn get_item_name_span(
    db: &dyn Db,
    file: baml_base::files::SourceFile,
    kind: &str,
    name: &str,
    occurrence: u16,
) -> Option<Span> {
    use baml_compiler_syntax::{
        SyntaxKind,
        ast::{
            ClassDef, ClientDef, EnumDef, FunctionDef, GeneratorDef, TemplateStringDef, TestDef,
            TypeAliasDef,
        },
    };

    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_id = file.file_id(db);
    let mut matches_found: u16 = 0;

    for node in tree.children() {
        match kind {
            "function" if node.kind() == SyntaxKind::FUNCTION_DEF => {
                if let Some(func) = FunctionDef::cast(node) {
                    if let Some(name_token) = func.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            // Also search for functions (methods) inside class definitions
            // Methods have qualified names like "ClassName.methodName"
            "function" if node.kind() == SyntaxKind::CLASS_DEF => {
                if let Some(class) = ClassDef::cast(node) {
                    let class_name = class.name().map(|n| n.text().to_string());
                    for method in class.methods() {
                        if let Some(name_token) = method.name() {
                            // Build qualified name and compare
                            let qualified_name = match &class_name {
                                Some(cn) => {
                                    QualifiedName::local_method_from_str(cn, name_token.text())
                                }
                                None => Name::new(name_token.text()),
                            };
                            if qualified_name.as_str() == name {
                                if matches_found == occurrence {
                                    return Some(Span::new(file_id, name_token.text_range()));
                                }
                                matches_found += 1;
                            }
                        }
                    }
                }
            }
            "class" if node.kind() == SyntaxKind::CLASS_DEF => {
                if let Some(class) = ClassDef::cast(node) {
                    if let Some(name_token) = class.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "enum" if node.kind() == SyntaxKind::ENUM_DEF => {
                if let Some(enum_def) = EnumDef::cast(node) {
                    if let Some(name_token) = enum_def.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "type alias" if node.kind() == SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(alias) = TypeAliasDef::cast(node) {
                    if let Some(name_token) = alias.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "client" if node.kind() == SyntaxKind::CLIENT_DEF => {
                if let Some(client) = ClientDef::cast(node) {
                    if let Some(name_token) = client.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "generator" if node.kind() == SyntaxKind::GENERATOR_DEF => {
                if let Some(generator) = GeneratorDef::cast(node) {
                    if let Some(name_token) = generator.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "test" if node.kind() == SyntaxKind::TEST_DEF => {
                if let Some(test) = TestDef::cast(node) {
                    if let Some(name_token) = test.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            "template_string" if node.kind() == SyntaxKind::TEMPLATE_STRING_DEF => {
                if let Some(ts) = TemplateStringDef::cast(node) {
                    if let Some(name_token) = ts.name() {
                        if name_token.text() == name {
                            if matches_found == occurrence {
                                return Some(Span::new(file_id, name_token.text_range()));
                            }
                            matches_found += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Validate that user-defined items don't use the reserved `stream_` prefix.
///
/// The `stream_` prefix is reserved for compiler-generated streaming types.
/// This checks the raw lowered items (before PPIR injection) so we only
/// flag user-authored items, not generated ones.
fn validate_stream_prefix(db: &dyn Db, root: baml_workspace::Project) -> Vec<HirDiagnostic> {
    let mut errors = Vec::new();

    for file in root.files(db) {
        // Check raw lowered items (without stream_* injection)
        let lowering = file_lowering(db, *file);
        let raw_tree = lowering.item_tree(db);

        for (id, class) in raw_tree.iter_classes() {
            if class.name.starts_with("stream_") {
                let span = get_item_name_span(db, *file, "class", &class.name, id.index())
                    .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                errors.push(HirDiagnostic::ReservedStreamPrefix {
                    item_kind: "class",
                    item_name: class.name.to_string(),
                    span,
                });
            }
        }

        for (id, alias) in raw_tree.iter_type_aliases() {
            if alias.name.starts_with("stream_") {
                let span = get_item_name_span(db, *file, "type alias", &alias.name, id.index())
                    .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                errors.push(HirDiagnostic::ReservedStreamPrefix {
                    item_kind: "type alias",
                    item_name: alias.name.to_string(),
                    span,
                });
            }
        }

        for (id, enum_def) in raw_tree.iter_enums() {
            if enum_def.name.starts_with("stream_") {
                let span = get_item_name_span(db, *file, "enum", &enum_def.name, id.index())
                    .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                errors.push(HirDiagnostic::ReservedStreamPrefix {
                    item_kind: "enum",
                    item_name: enum_def.name.to_string(),
                    span,
                });
            }
        }
    }

    errors
}

/// Validate that field names and function parameters don't use reserved keywords.
///
/// This checks:
/// - Class field names against reserved keywords in target languages
/// - Enum variant names against reserved keywords
/// - Function parameter names against reserved keywords
/// - Field names that match their type name (Python-specific issue)
///
/// The validation is based on which generators are configured in the project.
fn validate_reserved_names(db: &dyn Db, root: baml_workspace::Project) -> Vec<HirDiagnostic> {
    use std::collections::HashSet;

    let items = project_items(db, root);
    let mut errors = Vec::new();

    // First, collect all output types from generators
    let mut output_types: HashSet<reserved_names::OutputType> = HashSet::new();
    for item in items.items(db) {
        if let ItemId::Generator(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let generator = &item_tree[loc.id(db)];

            if let Some(ref output_type_str) = generator.output_type {
                if let Some(output_type) = reserved_names::OutputType::parse(output_type_str) {
                    output_types.insert(output_type);
                }
            }
        }
    }

    // If no generators, nothing to check
    if output_types.is_empty() {
        return errors;
    }

    // Get reserved names for field names
    let reserved_field_names =
        reserved_names::reserved_names_for_outputs(&output_types, ReservedNamesMode::FieldNames);

    // Get reserved names for function parameters
    let reserved_param_names = reserved_names::reserved_names_for_outputs(
        &output_types,
        ReservedNamesMode::FunctionParameters,
    );

    // Check if Python is a target (for field name == type name check)
    let has_python = output_types.contains(&reserved_names::OutputType::PythonPydantic);

    // Check class fields
    for item in items.items(db) {
        if let ItemId::Class(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let class = &item_tree[loc.id(db)];
            let class_name = class.name.as_str();

            for field in &class.fields {
                let field_name = field.name.as_str();

                // Get field info from syntax tree
                let field_info = get_class_field_info(db, file, class_name, field_name);
                let field_span = field_info
                    .as_ref()
                    .map(|info| info.span)
                    .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let has_alias = field_info
                    .as_ref()
                    .map(|info| info.has_alias)
                    .unwrap_or(false);

                // Check if field name is a reserved keyword
                if let Some(languages) = reserved_field_names.get(field_name) {
                    let target_languages: Vec<String> = languages
                        .iter()
                        .map(|l| l.display_name().to_string())
                        .collect();

                    errors.push(HirDiagnostic::ReservedFieldName {
                        item_kind: "class",
                        item_name: class_name.to_string(),
                        field_name: field_name.to_string(),
                        span: field_span,
                        target_languages,
                    });
                }

                // Check if field name matches its type name (Python-specific)
                // Skip if field has an @alias attribute
                if has_python && !has_alias {
                    if let Some(type_name) = get_base_type_name(&field.type_ref) {
                        // Compare case-sensitively - only error if exactly the same
                        if field_name == type_name {
                            errors.push(HirDiagnostic::FieldNameMatchesTypeName {
                                class_name: class_name.to_string(),
                                field_name: field_name.to_string(),
                                type_name: type_name.clone(),
                                span: field_span,
                            });
                        }
                    }
                }
            }
        }
    }

    // Check enum variants
    for item in items.items(db) {
        if let ItemId::Enum(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let enum_def = &item_tree[loc.id(db)];
            let enum_name = enum_def.name.as_str();

            for variant in &enum_def.variants {
                let variant_name = variant.name.as_str();

                // Get variant info from syntax tree
                let variant_info = get_enum_variant_info(db, file, enum_name, variant_name);
                let variant_span = variant_info
                    .as_ref()
                    .map(|info| info.span)
                    .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())));
                let has_alias = variant_info
                    .as_ref()
                    .map(|info| info.has_alias)
                    .unwrap_or(false);

                // Skip if variant has an @alias attribute
                if has_alias {
                    continue;
                }

                // Check if variant name is a reserved keyword
                if let Some(languages) = reserved_field_names.get(variant_name) {
                    let target_languages: Vec<String> = languages
                        .iter()
                        .map(|l| l.display_name().to_string())
                        .collect();

                    errors.push(HirDiagnostic::ReservedFieldName {
                        item_kind: "enum",
                        item_name: enum_name.to_string(),
                        field_name: variant_name.to_string(),
                        span: variant_span,
                        target_languages,
                    });
                }
            }
        }
    }

    // Check function parameters (skip synthetic render_prompt/build_request variants)
    for item in items.items(db) {
        if let ItemId::Function(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[loc.id(db)];
            if matches!(
                &func.compiler_generated,
                Some(
                    item_tree::CompilerGenerated::LlmRenderPrompt { .. }
                        | item_tree::CompilerGenerated::LlmBuildRequest { .. }
                )
            ) {
                continue;
            }
            let sig = function_signature(db, *loc);

            for param in &sig.params {
                let param_name = param.name.as_str();

                // Check if parameter name is a reserved keyword
                if let Some(languages) = reserved_param_names.get(param_name) {
                    let target_languages: Vec<String> = languages
                        .iter()
                        .map(|l| l.display_name().to_string())
                        .collect();

                    errors.push(HirDiagnostic::ReservedFieldName {
                        item_kind: "function",
                        item_name: func.name.to_string(),
                        field_name: param_name.to_string(),
                        span: Span::new(file.file_id(db), TextRange::empty(0.into())),
                        target_languages,
                    });
                }
            }
        }
    }

    errors
}

//
// ──────────────────────────────────────────────────── DEFINITION SPANS ─────
//

/// Returns the span of a definition's name in the source code.
///
/// Walks the AST to find the item definition and returns the text range of
/// its name token. Used by IDE features like goto-definition.
///
/// Note: This is not a Salsa tracked function because `Definition` is a plain
/// enum, not a Salsa struct. However, the underlying queries (item trees,
/// syntax trees) are cached, so this is still efficient.
pub fn definition_name_span(db: &dyn Db, def: Definition<'_>) -> Span {
    let (file, kind, name, index) = match def {
        Definition::Function(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "function",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::Class(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "class",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::Enum(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "enum",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::TypeAlias(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "type alias",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::Client(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "client",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::Generator(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "generator",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::Test(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "test",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
        Definition::TemplateString(loc) => {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            (
                file,
                "template_string",
                item_tree[loc.id(db)].name.clone(),
                loc.id(db).index(),
            )
        }
    };

    get_item_name_span(db, file, kind, &name, index)
        .unwrap_or_else(|| Span::new(file.file_id(db), TextRange::empty(0.into())))
}

/// Returns the span of a class field's name in the source code.
///
/// Walks the AST to find the class definition and then the field within it.
/// Returns the text range of the field's name token.
pub fn class_field_name_span(
    db: &dyn Db,
    class_loc: ClassLoc<'_>,
    field_name: &str,
) -> Option<Span> {
    use baml_compiler_syntax::{SyntaxKind, ast::ClassDef};

    let file = class_loc.file(db);
    let file_id = file.file_id(db);
    let item_tree = file_item_tree(db, file);
    let class_data = &item_tree[class_loc.id(db)];
    let class_name = class_data.name.as_str();
    let occurrence = class_loc.id(db).index();

    let tree = baml_compiler_parser::syntax_tree(db, file);
    let mut matches_found: u16 = 0;

    for node in tree.children() {
        if node.kind() == SyntaxKind::CLASS_DEF {
            if let Some(class) = ClassDef::cast(node) {
                if let Some(name_token) = class.name() {
                    if name_token.text() == class_name {
                        if matches_found == occurrence {
                            // Found the right class, now find the field
                            for field in class.fields() {
                                if let Some(field_name_token) = field.name() {
                                    if field_name_token.text() == field_name {
                                        return Some(Span::new(
                                            file_id,
                                            field_name_token.text_range(),
                                        ));
                                    }
                                }
                            }
                            return None; // Class found but field not found
                        }
                        matches_found += 1;
                    }
                }
            }
        }
    }

    None
}
