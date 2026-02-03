//! Fully Qualified Names for unambiguous item identification.
//!
//! This module re-exports the canonical `QualifiedName` from `baml_base`
//! and provides a type alias `FullyQualifiedName` for backward compatibility.

// Re-export the canonical types from baml_base
pub use baml_base::{Namespace, QualifiedName};

/// Type alias for backward compatibility.
///
/// New code should use `QualifiedName` directly.
pub type FullyQualifiedName = QualifiedName;
