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

use baml_base::{FileId, Name, SourceFile, Span};
use baml_diagnostics::{HirDiagnostic, NameError};
use baml_parser::syntax_tree;
use baml_syntax::SyntaxNode;
use rowan::{SyntaxToken, TextRange, ast::AstNode};

// Module declarations
mod body;
mod client;
mod generator;
mod generics;
mod ids;
mod item_tree;
mod loc;
mod path;
pub mod pretty;
pub mod reserved_names;
mod signature;
mod type_ref;

// Re-exports
pub use body::*;
pub use generics::*;
pub use ids::*;
pub use item_tree::*;
pub use loc::*;
pub use path::*;
pub use pretty::{body_to_code, expr_to_code, stmt_to_code};
pub use reserved_names::{OutputType, ReservedNamesMode};
// Re-export signature types explicitly (no wildcards to avoid conflicts)
pub use signature::{FunctionSignature, Param};
pub use type_ref::*;

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for HIR queries.
///
/// Extends `baml_workspace::Db`. Use the free functions in this crate
/// (e.g., `project_items`, `file_items`) for HIR queries.
#[salsa::db]
pub trait Db: baml_workspace::Db {}

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
/// This is a convenience wrapper around `file_lowering` for callers that
/// only need the `ItemTree`. Not tracked separately since `file_lowering`
/// already caches the result - this just clones the Arc (O(1)).
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    file_lowering(db, file).item_tree(db).clone()
}

// Future: When we add modules, we'll need a function like this:
// #[salsa::tracked]
// pub fn container_item_tree(db: &dyn Db, container: ContainerId) -> Arc<ItemTree>

/// Tracked: Get all items defined in a file.
///
/// Returns a tracked struct containing interned IDs for all top-level items.
#[salsa::tracked]
pub fn file_items(db: &dyn Db, file: SourceFile) -> FileItems<'_> {
    let item_tree = file_item_tree(db, file);
    let items = intern_all_items(db, file, &item_tree);
    FileItems::new(db, items)
}

/// Tracked: Get all items in the entire project.
#[salsa::tracked]
pub fn project_items(db: &dyn Db, root: baml_workspace::Project) -> ProjectItems<'_> {
    let mut all_items = Vec::new();

    for file in root.files(db) {
        let items_struct = file_items(db, *file);
        all_items.extend(items_struct.items(db).iter().copied());
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
#[salsa::tracked]
pub fn function_signature<'db>(
    db: &'db dyn Db,
    function: FunctionLoc<'db>,
) -> Arc<FunctionSignature> {
    let file = function.file(db);
    let tree = syntax_tree(db, file);
    let source_file = baml_syntax::ast::SourceFile::cast(tree).unwrap();

    // Find the function node by name
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    let func_name = func.name.clone();

    let default_signature = Arc::new(FunctionSignature {
        name: func.name.clone(),
        params: vec![],
        return_type: TypeRef::Unknown,
        return_type_span: None,
    });

    let function_def = source_file.items().find_map(|item| match item {
        baml_syntax::ast::Item::Function(func_node) => {
            let func_node_name = func_node.name();
            if func_node_name.as_ref()?.text() == func_name {
                Some(FunctionSignature::lower(&func_node))
            } else {
                None
            }
        }
        baml_syntax::ast::Item::Class(class_node) => class_node.methods().find_map(|method| {
            let method_name = method.name()?;
            let class_name = class_node.name();
            let class_name_text = class_name.as_ref()?.text();
            if method_name.text() == func_name {
                Some(lower_method_signature(&method, &func_name, class_name_text))
            } else {
                None
            }
        }),
        _ => None,
    });

    function_def.unwrap_or(default_signature)
}

/// Lower a method signature, replacing 'self' parameter with the class type.
fn lower_method_signature(
    method_node: &baml_syntax::ast::FunctionDef,
    method_name: &Name,
    class_name: &str,
) -> Arc<FunctionSignature> {
    // Extract parameters, replacing 'self' with the class type
    let mut params = Vec::new();
    if let Some(param_list) = method_node.param_list() {
        for param_node in param_list.params() {
            if let Some(name_token) = param_node.name() {
                let param_name = name_token.text();
                let type_ref = if param_name == "self" {
                    // 'self' gets the class type
                    TypeRef::named(class_name.into())
                } else {
                    param_node
                        .ty()
                        .map(|t| TypeRef::from_ast(&t))
                        .unwrap_or(TypeRef::Unknown)
                };

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
        .unwrap_or(TypeRef::Unknown);
    let return_type_span = return_type_node.map(|t| t.text_range());

    Arc::new(FunctionSignature {
        name: method_name.clone(),
        params,
        return_type,
        return_type_span,
    })
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
            let source_file = baml_syntax::ast::SourceFile::cast(tree).unwrap();
            let file_id = file.file_id(db);

            // Find the function in the CST to get its name span
            let span = source_file
                .items()
                .flat_map(|item| match item {
                    baml_syntax::ast::Item::Function(func_node) => vec![func_node],
                    baml_syntax::ast::Item::Class(class_node) => class_node.methods().collect(),
                    _ => vec![],
                })
                .find(|function_def| {
                    function_def.name().as_ref().map(SyntaxToken::text) == Some(&func_name)
                })
                .and_then(|f| f.name())
                .map(|name_token| Span::new(file_id, name_token.text_range()))
                .unwrap_or_else(|| Span::new(file_id, TextRange::empty(0.into())));

            functions.push((func_name.to_string(), span));
        }
    }

    functions
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
    let file = function.file(db);
    let tree = syntax_tree(db, file);
    let source_file = baml_syntax::ast::SourceFile::cast(tree).unwrap();

    let item_tree = file_item_tree(db, file);
    let func = &item_tree[function.id(db)];
    let func_name = func.name.clone();

    // Find the function among the top-level functions and class methods.
    let function_def = source_file
        .items()
        .flat_map(|item| match item {
            baml_syntax::ast::Item::Function(func_node) => vec![func_node],
            baml_syntax::ast::Item::Class(class_node) => class_node.methods().collect(),
            _ => vec![],
        })
        .find(|function_def| {
            function_def.name().as_ref().map(SyntaxToken::text) == Some(&func_name)
        });

    // Lower the function with file_id for span tracking.
    let file_id = file.file_id(db);
    function_def.map_or(Arc::new(FunctionBody::Missing), |f| {
        FunctionBody::lower(&f, file_id)
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
    use baml_syntax::SyntaxKind;

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
            if let Some(func) = lower_function(node) {
                tree.alloc_function(func);
            }
        }
        SyntaxKind::TYPE_ALIAS_DEF => {
            if let Some(alias) = lower_type_alias(node) {
                tree.alloc_type_alias(alias);
            }
        }
        SyntaxKind::CLIENT_DEF => {
            if let Some(c) = client::lower_client(node, ctx) {
                tree.alloc_client(c);
            }
        }
        SyntaxKind::TEST_DEF => {
            if let Some(test) = lower_test(node) {
                tree.alloc_test(test);
            }
        }
        SyntaxKind::GENERATOR_DEF => {
            if let Some(g) = generator::lower_generator(node, ctx) {
                tree.alloc_generator(g);
            }
        }
        _ => {
            // Skip other nodes (whitespace, comments, etc.)
        }
    }
}

/// Extract class definition from CST with validation.
fn lower_class(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Class> {
    use baml_syntax::ast::ClassDef;

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

            // Validate field attributes for duplicates
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
                        seen_field_attrs.insert(attr_name, attr_span);
                    }
                }
            }

            let type_ref = field_node
                .ty()
                .map(|t| TypeRef::from_ast(&t))
                .unwrap_or(TypeRef::Unknown);

            fields.push(crate::Field {
                name: field_name,
                type_ref,
            });
        }
    }

    // Track seen block attributes for duplicate detection
    let mut seen_attrs: FxHashMap<String, Span> = FxHashMap::default();
    let mut is_dynamic = false;

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

            // Track is_dynamic
            if attr_name == "dynamic" {
                is_dynamic = true;
            }
        }
    }

    Some(Class {
        name,
        fields,
        is_dynamic,
    })
}

/// Extract desugared method functions from a class.
/// Methods like `class Baz { function Greeting(self) }` become top-level functions `Greeting(self: Baz)`.
/// The method name is NOT namespaced - this keeps HIR lowering simple and type-free.
fn lower_class_methods(node: &SyntaxNode) -> Vec<Function> {
    use baml_syntax::ast::ClassDef;

    let Some(class) = ClassDef::cast(node.clone()) else {
        return Vec::new();
    };

    let mut functions = Vec::new();
    for method_node in class.methods() {
        if let Some(method_name) = method_node.name() {
            // Use just the method name (not qualified with class name)
            // This keeps HIR lowering simple - no type resolution needed
            functions.push(Function {
                name: method_name.text().into(),
            });
        }
    }
    functions
}

/// Extract enum definition from CST with validation.
fn lower_enum(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Enum> {
    use baml_syntax::ast::EnumDef;

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
                        seen_variant_attrs.insert(attr_name, attr_span);
                    }
                }
            }

            variants.push(crate::EnumVariant { name: variant_name });
        }
    }

    // Track seen block attributes for duplicate detection
    let mut seen_attrs: FxHashMap<String, Span> = FxHashMap::default();

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
                seen_attrs.insert(attr_name, attr_span);
            }
        }
    }

    Some(Enum { name, variants })
}

/// Extract function definition from CST - MINIMAL VERSION.
/// Only extracts the name. Signature and body are in separate queries.
fn lower_function(node: &SyntaxNode) -> Option<Function> {
    use baml_syntax::ast::FunctionDef;

    let func = FunctionDef::cast(node.clone())?;
    let name = func.name()?.text().into();

    Some(Function { name })
}

/// Extract type alias from CST.
fn lower_type_alias(node: &SyntaxNode) -> Option<TypeAlias> {
    use baml_syntax::ast::TypeAliasDef;

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
        .unwrap_or(TypeRef::Unknown);

    Some(TypeAlias { name, type_ref })
}

/// Extract test definition from CST.
fn lower_test(node: &SyntaxNode) -> Option<Test> {
    use baml_syntax::ast::TestDef;

    let test = TestDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name = test
        .name()
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("UnnamedTest"));

    // Extract all function references using AST accessor
    let function_refs = test
        .function_names()
        .into_iter()
        .map(|t| Name::new(t.text()))
        .collect();

    Some(Test {
        name,
        function_refs,
    })
}

//
// ────────────────────────────────────────────────────── NAME VALIDATION ─────
//

use rustc_hash::FxHashMap;

/// Information about a named item for duplicate detection.
struct ItemInfo {
    span: Span,
    path: String,
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
    HirValidationResult {
        hir_diagnostics: validate_reserved_names(db, root),
        name_errors: validate_duplicate_names(db, root),
    }
}

/// Validate that there are no duplicate names in the project.
///
/// Top-level entities (classes, enums, functions, type aliases, clients)
/// share the same namespace, so any duplicate name is an error.
///
/// Tests are validated separately: only tests with the same name AND
/// targeting the same function are considered duplicates.
fn validate_duplicate_names(db: &dyn Db, root: baml_workspace::Project) -> Vec<NameError> {
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
                let func = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    func.name.clone(),
                    "function",
                    span,
                    path,
                );
            }
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    class.name.clone(),
                    "class",
                    span,
                    path,
                );
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    enum_def.name.clone(),
                    "enum",
                    span,
                    path,
                );
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    alias.name.clone(),
                    "type alias",
                    span,
                    path,
                );
            }
            ItemId::Client(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let client = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    client.name.clone(),
                    "client",
                    span,
                    path,
                );
            }
            ItemId::Generator(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let generator = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                check_duplicate(
                    &mut seen,
                    &mut errors,
                    generator.name.clone(),
                    "generator",
                    span,
                    path,
                );
            }
            ItemId::Test(loc) => {
                // Tests are validated separately: only same name + same function is a duplicate
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let test = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
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
                            },
                        );
                    }
                }
            }
        }
    }

    errors
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
    if let Some(existing) = seen.get(&name) {
        errors.push(NameError::DuplicateName {
            name: name.to_string(),
            kind,
            first: existing.span,
            first_path: existing.path.clone(),
            second: span,
            second_path: path,
        });
    } else {
        seen.insert(name, ItemInfo { span, path });
    }
}

/// Extract the base type name from a `TypeRef`, unwrapping Optional, List, etc.
fn get_base_type_name(type_ref: &TypeRef) -> Option<String> {
    match type_ref {
        TypeRef::Path(path) => path.last_segment().map(std::string::ToString::to_string),
        TypeRef::Optional(inner) => get_base_type_name(inner),
        TypeRef::List(inner) => get_base_type_name(inner),
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
    use baml_syntax::{SyntaxKind, ast::ClassDef};

    let tree = baml_parser::syntax_tree(db, file);

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
    use baml_syntax::{SyntaxKind, ast::EnumDef};

    let tree = baml_parser::syntax_tree(db, file);

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
                        // Compare case-insensitively for Python
                        if field_name.to_lowercase() == type_name.to_lowercase() {
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

    // Check function parameters
    for item in items.items(db) {
        if let ItemId::Function(loc) = item {
            let file = loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[loc.id(db)];
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
