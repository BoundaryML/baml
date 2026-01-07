//! Re-exports all compiler crate APIs for convenience.
//!
//! This crate provides a single import point for all BAML compiler functionality.
//! For the main database type, use `baml_project::ProjectDatabase`.
//!
//! ## Usage
//!
//! ```ignore
//! use baml_db::{FileId, SourceFile, baml_hir, baml_parser};
//! ```

// Re-export all public APIs
pub use baml_base::*;
pub use baml_codegen;
pub use baml_diagnostics;
pub use baml_hir;
pub use baml_lexer;
pub use baml_mir;
pub use baml_parser;
pub use baml_syntax;
pub use baml_tir;
pub use baml_vir;
pub use baml_workspace;
pub use salsa::Setter;
