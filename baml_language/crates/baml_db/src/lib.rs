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

// Re-export function queries from baml_hir
pub use baml_hir::{function_body, function_signature};

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
