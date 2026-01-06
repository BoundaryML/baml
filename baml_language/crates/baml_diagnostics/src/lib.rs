//! BAML Diagnostics - Unified diagnostic types and rendering.
//!
//! This crate provides:
//! - A unified `Diagnostic` type that can represent any compiler error
//! - The `ToDiagnostic` trait for converting error types to `Diagnostic`
//! - Multi-format rendering (Ariadne for CLI, LSP types for editors)
//!
//! ## Architecture
//!
//! Following ty's design pattern, all compiler phases produce their own error
//! types (`ParseError`, `TypeError`, etc.) but they all implement `ToDiagnostic`
//! to convert to a unified `Diagnostic` type. This enables:
//!
//! - Centralized diagnostic collection via `Project::check()`
//! - Multi-format rendering without duplication
//! - Consistent error handling across all compiler phases

pub mod compiler_error;
pub mod diagnostic;
pub mod lsp;
pub mod render;
pub mod to_diagnostic;

// Re-export the unified diagnostic types
// Re-export the legacy error types and rendering (for backwards compatibility during migration)
pub use compiler_error::{
    ColorMode, CompilerError, HirDiagnostic, NameError, ParseError, TypeError, render_error,
    render_hir_diagnostic, render_name_error, render_parse_error, render_report_to_string,
    render_type_error,
};
pub use diagnostic::{Annotation, Diagnostic, DiagnosticId, RelatedInfo, Severity, ToDiagnostic};
// Re-export LSP conversion utilities
pub use lsp::{LspConversionConfig, compute_line_starts};
// Re-export the rendering functions
pub use render::{DiagnosticFormat, RenderConfig, render_diagnostic, render_diagnostics};
