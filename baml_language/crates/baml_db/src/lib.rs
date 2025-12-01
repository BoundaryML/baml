//! Root database that assembles all compiler phases.
//!
//! This crate purely combines all the compiler traits into a single database.
//! All testing happens in the separate `baml_tests` crate.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

// Re-export all public APIs
pub use baml_base::*;
pub use baml_codegen;
pub use baml_diagnostics;
pub use baml_hir;
pub use baml_lexer;
pub use baml_parser;
pub use baml_syntax;
pub use baml_thir;
pub use baml_workspace;
use rowan::ast::AstNode;
use salsa::Storage;

/// Type alias for Salsa event callbacks
pub type EventCallback = Box<dyn Fn(salsa::Event) + Send + Sync + 'static>;

/// Root database combining all compiler phases.
/// With Salsa 2022, we use the #[`salsa::db`] attribute
#[salsa::db]
#[derive(Clone)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: std::sync::Arc<AtomicU32>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl baml_hir::Db for RootDatabase {}

#[salsa::db]
impl baml_thir::Db for RootDatabase {}

impl RootDatabase {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self {
            storage: Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Create a new database with an event callback for tracking query execution.
    ///
    /// The callback will be invoked for various Salsa events, including:
    /// - `WillExecute`: A query is about to be recomputed
    /// - `DidValidateMemoizedValue`: A cached value was reused
    ///
    /// This is useful for tracking incremental compilation behavior.
    pub fn new_with_event_callback(callback: EventCallback) -> Self {
        Self {
            storage: Storage::new(Some(callback)),
            next_file_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Add a file to the database.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );

        // Create a new SourceFile input
        SourceFile::new(self, text.into(), path.into(), file_id)
    }

    /// Create a project root
    pub fn set_project_root(&mut self, path: impl Into<PathBuf>) -> baml_workspace::ProjectRoot {
        baml_workspace::ProjectRoot::new(self, path.into())
    }
}

impl Default for RootDatabase {
    fn default() -> Self {
        Self::new()
    }
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
    db: &'db dyn baml_hir::Db,
    file: SourceFile,
    function: baml_hir::FunctionLoc<'db>,
) -> Arc<baml_hir::FunctionSignature> {
    let tree = baml_parser::syntax_tree(db, file);
    let source_file = baml_syntax::ast::SourceFile::cast(tree).unwrap();

    // Find the function node by name
    let item_tree = baml_hir::file_item_tree(db, file);
    let func = &item_tree[function.id(db)];

    // First, look for a top-level function
    for item in source_file.items() {
        if let baml_syntax::ast::Item::Function(func_node) = item {
            if let Some(name_token) = func_node.name() {
                if name_token.text() == func.name.as_str() {
                    return baml_hir::FunctionSignature::lower(&func_node);
                }
            }
        }
    }

    // Then, look for a method inside classes (methods are desugared to top-level functions)
    for item in source_file.items() {
        if let baml_syntax::ast::Item::Class(class_node) = item {
            if let Some(class_name_token) = class_node.name() {
                let class_name = class_name_token.text();
                for method_node in class_node.methods() {
                    if let Some(method_name_token) = method_node.name() {
                        if method_name_token.text() == func.name.as_str() {
                            return lower_method_signature(&method_node, &func.name, class_name);
                        }
                    }
                }
            }
        }
    }

    // Function not found - return minimal signature
    Arc::new(baml_hir::FunctionSignature {
        name: func.name.clone(),
        params: vec![],
        return_type: baml_hir::TypeRef::Unknown,
    })
}

/// Lower a method signature, replacing 'self' parameter with the class type.
fn lower_method_signature(
    method_node: &baml_syntax::ast::FunctionDef,
    method_name: &baml_base::Name,
    class_name: &str,
) -> Arc<baml_hir::FunctionSignature> {
    use baml_hir::{FunctionSignature, Param, TypeRef};

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
                        .map(|t| baml_hir::lower_type_ref(&t))
                        .unwrap_or(TypeRef::Unknown)
                };

                params.push(Param {
                    name: baml_base::Name::new(param_name),
                    type_ref,
                });
            }
        }
    }

    // Extract return type
    let return_type = method_node
        .return_type()
        .map(|t| baml_hir::lower_type_ref(&t))
        .unwrap_or(TypeRef::Unknown);

    Arc::new(FunctionSignature {
        name: method_name.clone(),
        params,
        return_type,
    })
}

/// Returns the body of a function (LLM prompt or expression IR).
///
/// This is the most frequently invalidated query - it changes whenever
/// the function body is edited.
#[salsa::tracked]
pub fn function_body<'db>(
    db: &'db dyn baml_hir::Db,
    file: SourceFile,
    function: baml_hir::FunctionLoc<'db>,
) -> Arc<baml_hir::FunctionBody> {
    let tree = baml_parser::syntax_tree(db, file);
    let source_file = baml_syntax::ast::SourceFile::cast(tree).unwrap();

    let item_tree = baml_hir::file_item_tree(db, file);
    let func = &item_tree[function.id(db)];

    // First, look for a top-level function
    for item in source_file.items() {
        if let baml_syntax::ast::Item::Function(func_node) = item {
            if let Some(name_token) = func_node.name() {
                if name_token.text() == func.name.as_str() {
                    return baml_hir::FunctionBody::lower(&func_node);
                }
            }
        }
    }

    // Then, look for a method inside classes
    for item in source_file.items() {
        if let baml_syntax::ast::Item::Class(class_node) = item {
            for method_node in class_node.methods() {
                if let Some(method_name_token) = method_node.name() {
                    if method_name_token.text() == func.name.as_str() {
                        return baml_hir::FunctionBody::lower(&method_node);
                    }
                }
            }
        }
    }

    // No body found
    Arc::new(baml_hir::FunctionBody::Missing)
}

//
// ────────────────────────────────────────────────── TYPING CONTEXT ─────
//

/// Build typing context from a list of source files.
///
/// This maps function names to their arrow types, e.g.:
/// `Foo` -> `(int) -> int` for `function Foo(x: int) -> int`
///
/// This is used as the starting scope when type-checking function bodies,
/// allowing function calls to be properly typed.
///
/// Note: This is not a Salsa query because it returns `Ty<'db>` which contains
/// lifetime-parameterized data. Callers should cache the result if needed.
pub fn build_typing_context_from_files<'db>(
    db: &'db dyn baml_thir::Db,
    files: &[SourceFile],
) -> std::collections::HashMap<baml_base::Name, baml_thir::Ty<'db>> {
    let mut context = std::collections::HashMap::new();

    for file in files {
        let items_struct = baml_hir::file_items(db, *file);
        let items = items_struct.items(db);

        for item in items {
            if let baml_hir::ItemId::Function(func_loc) = item {
                let signature = function_signature(db, *file, *func_loc);

                // Build the arrow type: (param_types) -> return_type
                let param_types: Vec<baml_thir::Ty<'db>> = signature
                    .params
                    .iter()
                    .map(|p| baml_thir::lower_type_ref(db, &p.type_ref))
                    .collect();

                let return_type = baml_thir::lower_type_ref(db, &signature.return_type);

                let func_type = baml_thir::Ty::Function {
                    params: param_types,
                    ret: Box::new(return_type),
                };

                context.insert(signature.name.clone(), func_type);
            }
        }
    }

    context
}

/// Build class fields map from a list of source files.
///
/// This maps class names to their field types, e.g.:
/// `Baz` -> { `name` -> `String` }
///
/// Used for field access type checking in tests and onionskin.
pub fn build_class_fields_from_files<'db>(
    db: &'db dyn baml_thir::Db,
    files: &[SourceFile],
) -> std::collections::HashMap<
    baml_base::Name,
    std::collections::HashMap<baml_base::Name, baml_thir::Ty<'db>>,
> {
    let mut class_fields = std::collections::HashMap::new();

    for file in files {
        let item_tree = baml_hir::file_item_tree(db, *file);

        // Iterate over all classes in the item tree
        for (_, class) in item_tree.iter_classes() {
            let mut fields = std::collections::HashMap::new();
            for field in &class.fields {
                let field_ty = baml_thir::lower_type_ref(db, &field.type_ref);
                fields.insert(field.name.clone(), field_ty);
            }
            class_fields.insert(class.name.clone(), fields);
        }
    }

    class_fields
}
