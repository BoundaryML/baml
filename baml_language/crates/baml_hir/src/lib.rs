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

use baml_base::{Name, SourceFile, Span};
use baml_diagnostics::NameError;
use baml_parser::syntax_tree;
use baml_syntax::SyntaxNode;
use rowan::{SyntaxToken, TextRange, ast::AstNode};

// Module declarations
mod body;
mod generics;
mod ids;
mod item_tree;
mod loc;
mod path;
pub mod pretty;
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
// Re-export signature types explicitly (no wildcards to avoid conflicts)
pub use signature::{FunctionSignature, Param};
pub use type_ref::*;

//
// ──────────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for HIR queries.
///
/// This is the base trait for the BAML compiler's Db hierarchy.
/// It provides access to the project being compiled and is extended by
/// downstream crates (`baml_thir::Db`, `baml_mir::Db`, etc.) to add
/// phase-specific queries.
///
/// The Db trait hierarchy starts here because this is the first compiler
/// phase that needs project context. Earlier phases (lexer, parser) only
/// need `salsa::Database` for interning/tracking.
#[salsa::db]
pub trait Db: salsa::Database {
    /// Returns the project being analyzed.
    ///
    /// The project contains all source files (both user files and dependencies)
    /// and is the root input for all queries.
    fn project(&self) -> baml_workspace::Project;
}

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

//
// ────────────────────────────────────────────────────────── SALSA QUERIES ─────
//

/// Tracked: Extract `ItemTree` from a file's syntax tree.
///
/// This query is the "invalidation barrier" - it only changes when
/// item signatures change, not when whitespace/comments/bodies change.
#[salsa::tracked]
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    let tree = syntax_tree(db, file);
    let item_tree = lower_file(&tree);
    Arc::new(item_tree)
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
    let files = baml_workspace::project_files(db, root);
    let mut all_items = Vec::new();

    for file in files {
        let items_struct = file_items(db, file);
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

    // Extract return type
    let return_type = method_node
        .return_type()
        .map(|t| TypeRef::from_ast(&t))
        .unwrap_or(TypeRef::Unknown);

    Arc::new(FunctionSignature {
        name: method_name.clone(),
        params,
        return_type,
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

/// Lower a syntax tree into an `ItemTree`.
///
/// This is the main extraction logic that walks the CST and builds
/// position-independent item representations.
fn lower_file(root: &SyntaxNode) -> ItemTree {
    let mut tree = ItemTree::new();

    // Walk only direct children of the root (top-level items)
    // Don't use descendants() because that would pick up nested items like
    // CLIENT_DEF nodes inside function bodies
    for child in root.children() {
        lower_item(&mut tree, &child);
    }

    tree
}

/// Lower a single item from the CST.
fn lower_item(tree: &mut ItemTree, node: &SyntaxNode) {
    use baml_syntax::SyntaxKind;

    match node.kind() {
        SyntaxKind::CLASS_DEF => {
            if let Some(class) = lower_class(node) {
                tree.alloc_class(class);
            }
            // Desugar methods into top-level functions
            for func in lower_class_methods(node) {
                tree.alloc_function(func);
            }
        }
        SyntaxKind::ENUM_DEF => {
            if let Some(enum_def) = lower_enum(node) {
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
            if let Some(client) = lower_client(node) {
                tree.alloc_client(client);
            }
        }
        SyntaxKind::TEST_DEF => {
            if let Some(test) = lower_test(node) {
                tree.alloc_test(test);
            }
        }
        _ => {
            // Skip other nodes (whitespace, comments, etc.)
        }
    }
}

/// Extract class definition from CST.
fn lower_class(node: &SyntaxNode) -> Option<Class> {
    use baml_syntax::ast::ClassDef;

    let class = ClassDef::cast(node.clone())?;
    let name = class.name()?.text().into();
    let mut fields = Vec::new();

    // Extract fields
    for field_node in class.fields() {
        if let Some(field_name) = field_node.name() {
            let type_ref = field_node
                .ty()
                .map(|t| TypeRef::from_ast(&t))
                .unwrap_or(TypeRef::Unknown);

            fields.push(crate::Field {
                name: field_name.text().into(),
                type_ref,
            });
        }
    }

    // Check for @@dynamic attribute using AST accessor
    let is_dynamic = class
        .block_attributes()
        .any(|attr| attr.name().map(|n| n.text() == "dynamic").unwrap_or(false));

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

/// Extract enum definition from CST.
fn lower_enum(node: &SyntaxNode) -> Option<Enum> {
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

    // Extract variants using AST accessor
    let variants = enum_def
        .variants()
        .filter_map(|variant| {
            variant.name().map(|name_token| crate::EnumVariant {
                name: Name::new(name_token.text()),
            })
        })
        .collect();

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

/// Extract client configuration from CST.
fn lower_client(node: &SyntaxNode) -> Option<Client> {
    use baml_syntax::ast::ClientDef;

    let client_def = ClientDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name = client_def
        .name()
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("UnnamedClient"));

    // Extract provider from config block using AST accessors
    let provider = client_def
        .config_block()
        .and_then(|block| {
            block
                .items()
                .find(|item| item.key().map(|k| k.text() == "provider").unwrap_or(false))
                .and_then(|item| item.value_word())
                .map(|t| Name::new(t.text()))
        })
        .unwrap_or_else(|| Name::new("unknown"));

    Some(Client { name, provider })
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

    // Extract function reference using AST accessor
    let function_refs = test
        .function_name()
        .map(|t| vec![Name::new(t.text())])
        .unwrap_or_default();

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

/// Validate that there are no duplicate names in the project.
///
/// All top-level entities (classes, enums, functions, type aliases, clients, tests)
/// share the same namespace, so any duplicate name is an error.
pub fn validate_duplicate_names(db: &dyn Db, root: baml_workspace::Project) -> Vec<NameError> {
    let items = project_items(db, root);
    let mut seen: FxHashMap<Name, ItemInfo> = FxHashMap::default();
    let mut errors = Vec::new();

    for item in items.items(db) {
        let (name, kind, span, path) = match item {
            ItemId::Function(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let func = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (func.name.clone(), "function", span, path)
            }
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (class.name.clone(), "class", span, path)
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (enum_def.name.clone(), "enum", span, path)
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (alias.name.clone(), "type alias", span, path)
            }
            ItemId::Client(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let client = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (client.name.clone(), "client", span, path)
            }
            ItemId::Test(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let test = &item_tree[loc.id(db)];
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));
                let path = file.path(db).display().to_string();
                (test.name.clone(), "test", span, path)
            }
        };

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

    errors
}
