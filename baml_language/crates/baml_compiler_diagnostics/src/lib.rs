//! BAML Diagnostics - Unified diagnostic types and rendering.
//!
//! This crate provides:
//! - A unified `Diagnostic` type that can represent any compiler error
//! - The `ToDiagnostic` trait for converting error types to `Diagnostic`
//! - Multi-format rendering (Miette for CLI)
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
//! let output = render_diagnostic(&diagnostic, &sources, &file_paths, &RenderConfig::default());
//! ```
//!
//! ## LSP Conversion
//!
//! LSP-specific conversion (to `lsp_types::Diagnostic`) lives in the
//! `lsp_server` crate, keeping this crate free of LSP dependencies.

pub mod diagnostic;
pub mod errors;
pub mod highlight;
pub mod message;
pub mod render;
pub mod to_diagnostic;

// Re-export error types
// Re-export the unified diagnostic types
pub use diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity, ToDiagnostic};
pub use errors::{ErrorContext, NameError, ParseError, TypeError};
pub use highlight::{
    DiagnosticMessageHighlighter, HighlightAttributes, HighlightColor, HighlightSpan,
    HighlightStyle, SourceHighlights,
};
pub use message::{
    DiagnosticIdentifierKind, DiagnosticMessageHighlight, DiagnosticMessageKind, DiagnosticText,
};
// Re-export the rendering functions and types
pub use render::{RenderConfig, render_diagnostic};
