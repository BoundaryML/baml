//! Project and workspace utilities for BAML LSP integration.
//!
//! This crate provides project-aware functionality like file tracking, symbol
//! listing, and position/span utilities for use by LSP servers and tests.
//!
//! ## Centralized Diagnostics
//!
//! The `LspDatabase::check()` method provides centralized diagnostic collection,
//! returning unified `Diagnostic` types that can be rendered in multiple formats.
//!
//! ```ignore
//! let mut db = LspDatabase::new();
//! db.set_project_root(path);
//! db.add_or_update_file(file_path, content);
//!
//! let (diagnostics, sources) = db.check();
//! for diag in diagnostics {
//!     let rendered = render_diagnostic(&diag, &sources, &RenderConfig::test());
//!     println!("{}", rendered);
//! }
//! ```

mod check;
mod lsp_db;

pub mod position;
pub mod symbols;

pub use check::{CheckResult, collect_diagnostics};
pub use lsp_db::LspDatabase;
pub use symbols::{
    Symbol, SymbolKind, find_symbol, find_symbol_locations, list_classes, list_clients, list_enums,
    list_functions, list_generators, list_tests, list_type_aliases,
};
