//! BAML Diagnostics - Unified diagnostic types and rendering.
//!
//! This crate provides:
//! - A unified `Diagnostic` type that can represent any compiler error
//! - The `ToDiagnostic` trait for converting error types to `Diagnostic`
//! - Multi-format rendering (Ariadne for CLI)
//!
//! ## Architecture
//!
//! Following ty's design pattern, all compiler phases produce their own error
//! types (`ParseError`, `TypeError`, etc.) but they all implement `ToDiagnostic`
//! to convert to a unified `Diagnostic` type. This enables:
//!
//! - Centralized diagnostic collection via `baml_project::collect_diagnostics()`
//! - Multi-format rendering without duplication
//! - Consistent error handling across all compiler phases
//!
//! ## LSP Conversion
//!
//! LSP-specific conversion (to `lsp_types::Diagnostic`) lives in the
//! `baml_language_server` crate, keeping this crate free of LSP dependencies.

pub mod compiler_error;
pub mod diagnostic;
pub mod render;
pub mod to_diagnostic;

// Re-export the unified diagnostic types
// Re-export the legacy error types and rendering (for backwards compatibility during migration)
pub use compiler_error::{
    ColorMode, CompilerError, DbSourceCache, HirDiagnostic, NameError, ParseError, TypeError,
    render_error, render_hir_diagnostic, render_name_error, render_parse_error,
    render_report_to_string, render_type_error,
};
pub use diagnostic::{
    Annotation, Diagnostic, DiagnosticId, DiagnosticPhase, RelatedInfo, Severity, ToDiagnostic,
};
// Re-export the rendering functions and types
pub use render::{
    DiagnosticFormat, RenderConfig, SourceCache, render_diagnostic, render_diagnostics,
};
