//! Core types and traits used throughout the BAML compiler.
//!
//! This crate has NO dependencies on other compiler crates to avoid circular dependencies.

pub mod client_options;
pub mod core_types;
pub mod dedent;
pub mod escape;
pub mod files;
pub mod lang;
pub mod language;
pub mod num_lit;
pub mod qualified_name;

// Re-export everything for convenience
pub use client_options::*;
pub use core_types::*;
pub use files::*;
pub use lang::*;
pub use language::*;
pub use qualified_name::{BAML_STD_PREFIX, Namespace, QualifiedName};

/// The reserved method name of a class's finalizer (BEP-042): a
/// `cleanup(self) -> void` the collector runs when an instance dies. The
/// compiler recognizes the shape by this name and records the method's
/// position in the class's table (`Class::cleanup`); the runtime reads the
/// position, never the name.
pub const CLEANUP_METHOD: &str = "cleanup";

/// Namespace within the `baml` package that contains panic classes.
pub const PANICS_NAMESPACE: &str = "panics";
