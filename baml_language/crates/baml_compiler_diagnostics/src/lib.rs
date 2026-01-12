//! BAML Diagnostics - Unified diagnostic types and rendering.
//!
//! This crate provides:
//! - A unified `Diagnostic` type that can represent any compiler error
//! - The `ToDiagnostic` trait for converting error types to `Diagnostic`
//! - Multi-format rendering (Ariadne for CLI)
//!
//! ## Architecture
//!
//! All compiler phases produce their own error types (`ParseError`, `TypeError`, etc.)
//! but they all implement `ToDiagnostic` to convert to a unified `Diagnostic` type.
//! This enables:
//!
//! - Centralized diagnostic collection via `baml_project::collect_diagnostics()`
//! - Multi-format rendering without duplication
//! - Consistent error handling across all compiler phases
//!
//! ## Usage
//!
//! ```ignore
//! use baml_compiler_diagnostics::{ParseError, ToDiagnostic, RenderConfig, render_diagnostic};
//!
//! let error = ParseError::UnexpectedToken { ... };
//! let diagnostic = error.to_diagnostic();
//! let output = render_diagnostic(&diagnostic, &sources, &file_paths, &RenderConfig::cli());
//! ```
//!
//! ## LSP Conversion
//!
//! LSP-specific conversion (to `lsp_types::Diagnostic`) lives in the
//! `baml_lsp` crate, keeping this crate free of LSP dependencies.

pub mod diagnostic;
pub mod errors;
pub mod render;
pub mod to_diagnostic;

// Re-export error types
// Re-export the unified diagnostic types
pub use diagnostic::{
    Annotation, Diagnostic, DiagnosticId, DiagnosticPhase, RelatedInfo, Severity, ToDiagnostic,
};
pub use errors::{HirDiagnostic, NameError, ParseError, TypeError};
// Re-export the rendering functions and types
pub use render::{
    DiagnosticFormat, RenderConfig, SourceCache, render_diagnostic, render_diagnostics,
};
