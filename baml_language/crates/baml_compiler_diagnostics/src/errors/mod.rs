//! Compiler error types for BAML.
//!
//! This module provides the structured error types used by different compiler phases.
//! Each error type implements `ToDiagnostic` to convert to the unified `Diagnostic` type.
//!
//! ## Error Types
//!
//! - [`ParseError`] - Syntax errors from the parser
//! - [`TypeError`] - Type checking errors (generic over the type representation)
//! - [`NameError`] - Name resolution errors (duplicates across files)
//! - [`HirDiagnostic`] - HIR lowering errors (per-file validation)

mod hir_diagnostic;
mod name_error;
mod parse_error;
mod type_error;

pub use hir_diagnostic::HirDiagnostic;
pub use name_error::NameError;
pub use parse_error::ParseError;
pub use type_error::TypeError;
