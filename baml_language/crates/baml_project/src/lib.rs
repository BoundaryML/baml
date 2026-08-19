//! Project and workspace utilities for BAML LSP integration.
//!
//! This crate provides project-aware functionality like file tracking, symbol
//! listing, and position/span utilities for use by LSP servers and tests.
//!
//! ## `ProjectDatabase`
//!
//! The main database type is `ProjectDatabase`, which owns the Salsa storage
//! directly (following the ty/ruff pattern) and provides centralized diagnostic
//! collection via the `check()` method.
//!
//! ```ignore
//! let mut db = ProjectDatabase::new();
//! db.set_project_root(path);
//! db.add_or_update_file(file_path, content);
//!
//! let result = db.check();
//! for diag in &result.diagnostics {
//!     let rendered = render_diagnostic(&diag, &result.sources, &result.file_paths, &config);
//!     println!("{}", rendered);
//! }
//! ```

mod check;
mod client_codegen;
mod db;

pub mod param_schema;
pub mod position;
pub mod symbols;
#[cfg(feature = "testing")]
pub mod testing;

pub use check::{
    CheckResult, NarrowedDiagnostics, check_files_parallel, collect_compiler2_diagnostics,
    collect_compiler2_diagnostics_narrowed, collect_diagnostics, collect_package_level_diagnostics,
    prime_file_indexes_parallel,
};
pub use client_codegen::{build_interface_implementors, build_symbol_pool};
pub use db::{CursorContext, EventCallback, ProjectDatabase};
pub use param_schema::{FieldSchema, FieldSchemaField, ParamSchema, TypeSchema};
pub use symbols::{
    FunctionListing, FunctionOrigin, FunctionSourcePosition, FunctionSymbol, Symbol, SymbolKind,
    TestSymbol, list_functions_with_metadata, list_tests_with_metadata,
};
