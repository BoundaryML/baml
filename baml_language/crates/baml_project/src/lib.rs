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
mod db;

pub mod position;
pub mod symbols;

pub use check::{CheckResult, collect_diagnostics};
pub use db::{EventCallback, ProjectDatabase};
pub use symbols::{
    Symbol, SymbolKind, find_symbol, find_symbol_locations, list_classes, list_clients, list_enums,
    list_functions, list_generators, list_tests, list_type_aliases,
};
