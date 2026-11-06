//! Compiler error types for BAML.
//!
//! - [`ParseError`] - Syntax errors from the parser, converted to the unified
//!   `Diagnostic` type via `ToDiagnostic`.

mod parse_error;

pub use parse_error::ParseError;
