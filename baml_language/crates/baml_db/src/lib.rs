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
///
/// Uses `AstId` for O(1) direct lookup instead of O(n) linear search.
///
/// Note: Takes only `function` as parameter (not `file` separately).
/// The file is retrieved from `function.file(db)`. This is critical for
/// incrementality - passing `file` separately would cause unnecessary re-runs.
#[salsa::tracked]
pub fn function_signature<'db>(
    db: &'db dyn baml_hir::Db,
    function: baml_hir::FunctionLoc<'db>,
) -> Arc<baml_hir::FunctionSignature> {
    // Get file from function location
    let file = function.file(db);

    // Get AstIdMap for stable node addressing
    let ast_id_map = baml_hir::file_ast_id_map(db, file);

    // Get function's AstId from location
    let ast_id = function.ast_id(db);

    // Use AstId to find node directly (O(1) lookup)
    let ptr = ast_id_map
        .get(ast_id)
        .expect("function AstId must exist in map");

    // Convert pointer to node
    let tree = baml_parser::syntax_tree(db, file);
    let func_node = ptr.to_node(&tree);
    let func_def =
        baml_syntax::ast::FunctionDef::cast(func_node).expect("AstId must point to function");

    // Lower signature
    baml_hir::FunctionSignature::lower(&func_def)
}

/// Extract and intern the body text of a function for content-based caching.
///
/// This query extracts just the body text and interns it. If the body text
/// is unchanged, the interned ID will be the same, allowing downstream
/// queries to be cached.
#[salsa::tracked]
pub fn function_body_text<'db>(
    db: &'db dyn baml_hir::Db,
    function: baml_hir::FunctionLoc<'db>,
) -> baml_hir::FunctionBodyText<'db> {
    // Get file from function location
    let file = function.file(db);

    // Get AstIdMap for stable node addressing
    let ast_id_map = baml_hir::file_ast_id_map(db, file);

    // Get function's AstId from location
    let ast_id = function.ast_id(db);

    // Use AstId to find node directly (O(1) lookup)
    let ptr = ast_id_map
        .get(ast_id)
        .expect("function AstId must exist in map");

    // Convert pointer to node
    let tree = baml_parser::syntax_tree(db, file);
    let func_node = ptr.to_node(&tree);
    let func_def =
        baml_syntax::ast::FunctionDef::cast(func_node).expect("AstId must point to function");

    // Extract body text (for content-based caching)
    let body_text = if let Some(llm_body) = func_def.llm_body() {
        llm_body.syntax().text().to_string()
    } else if let Some(expr_body) = func_def.expr_body() {
        expr_body.syntax().text().to_string()
    } else {
        String::new()
    };

    // Intern the body text - if unchanged, same ID returned
    baml_hir::FunctionBodyText::new(db, body_text)
}

/// Lower a function body based on its interned body text (internal query).
///
/// This query takes BOTH the function location AND the interned body text.
/// By including `body_text` as a parameter, Salsa can cache based on it:
/// if `body_text` is unchanged, the cached result is returned.
#[salsa::tracked]
fn function_body_impl<'db>(
    db: &'db dyn baml_hir::Db,
    function: baml_hir::FunctionLoc<'db>,
    _body_text: baml_hir::FunctionBodyText<'db>,
) -> Arc<baml_hir::FunctionBody> {
    // Get file from function location
    let file = function.file(db);

    // Get AstIdMap for stable node addressing
    let ast_id_map = baml_hir::file_ast_id_map(db, file);

    // Get function's AstId from location
    let ast_id = function.ast_id(db);

    // Use AstId to find node directly (O(1) lookup)
    let ptr = ast_id_map
        .get(ast_id)
        .expect("function AstId must exist in map");

    // Convert pointer to node
    let tree = baml_parser::syntax_tree(db, file);
    let func_node = ptr.to_node(&tree);
    let func_def =
        baml_syntax::ast::FunctionDef::cast(func_node).expect("AstId must point to function");

    // Lower body
    // Even though we access the syntax tree here, Salsa will cache the result
    // based on the body_text parameter. If body_text is unchanged, we return
    // the cached Arc (same pointer).
    baml_hir::FunctionBody::lower(&func_def)
}

/// Returns the body of a function (LLM prompt or expression IR).
///
/// This is the public API. It internally calls `function_body_text` and
/// `function_body_impl` to enable content-based caching.
pub fn function_body<'db>(
    db: &'db dyn baml_hir::Db,
    function: baml_hir::FunctionLoc<'db>,
) -> Arc<baml_hir::FunctionBody> {
    let body_text = function_body_text(db, function);
    function_body_impl(db, function, body_text)
}
