//! Root database that assembles all compiler phases.
//!
//! This crate purely combines all the compiler traits into a single database.

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
pub use baml_mir;
pub use baml_parser;
pub use baml_syntax;
pub use baml_thir;
pub use baml_workspace;
pub use salsa::Setter;
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

#[salsa::db]
impl baml_mir::Db for RootDatabase {}

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

    /// Create a project root with an empty file list.
    ///
    /// After creating the project root, use `add_file()` to add source files,
    /// then update the project root's file list with `root.set_files()`.
    pub fn set_project_root(&mut self, path: impl Into<PathBuf>) -> baml_workspace::Project {
        baml_workspace::Project::new(self, path.into(), vec![])
    }
}

impl Default for RootDatabase {
    fn default() -> Self {
        Self::new()
    }
}
