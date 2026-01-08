//! Re-exports all compiler crate APIs for convenience.
//!
//! This crate provides a single import point for all BAML compiler functionality.
//! For the main database type, use `baml_project::ProjectDatabase`.
//!
//! ## Usage
//!
//! ```ignore
//! use baml_db::{FileId, SourceFile, baml_compiler_hir, baml_compiler_parser};
//! ```

// Re-export all public APIs
pub use baml_base::*;
pub use baml_compiler_diagnostics;
pub use baml_compiler_emit;
pub use baml_compiler_hir;
pub use baml_compiler_lexer;
pub use baml_compiler_mir;
pub use baml_compiler_parser;
pub use baml_compiler_syntax;
pub use baml_compiler_tir;
pub use baml_compiler_vir;
pub use baml_workspace;
pub use salsa::Setter;
