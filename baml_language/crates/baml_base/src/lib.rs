//! Core types and traits used throughout the BAML compiler.
//!
//! This crate has NO dependencies on other compiler crates to avoid circular dependencies.

pub mod attr;
pub mod client_options;
pub mod core_types;
pub mod debug_log;
pub mod dedent;
pub mod escape;
pub mod files;
pub mod num_lit;
pub mod qualified_name;
pub mod salsa_update;

// Re-export everything for convenience
pub use attr::*;
pub use client_options::*;
pub use core_types::*;
pub use debug_log::{DebugMessage, drain_debug_log, has_debug_messages};
pub use files::*;
pub use qualified_name::{BAML_STD_PREFIX, Namespace, QualifiedName};

/// Package name for the BAML standard library / builtins.
pub const BAML_PACKAGE: &str = "baml";

/// Namespace within the `baml` package that contains panic classes.
pub const PANICS_NAMESPACE: &str = "panics";

/// Check whether a (package, namespace) pair refers to the `baml.panics` namespace.
pub fn is_panic_namespace(package: &str, namespace: &[Name]) -> bool {
    package == BAML_PACKAGE && namespace.len() == 1 && namespace[0].as_str() == PANICS_NAMESPACE
}
