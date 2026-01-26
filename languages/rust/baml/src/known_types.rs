//! `KnownTypes` trait for project-specific type enums.
//!
//! This is a standalone module at crate root to avoid circular dependencies
//! between error.rs and codec/.

use std::any::Any;

/// Trait for project-specific known types enum.
/// Implemented by `CodeGen`'d `Types` and `StreamTypes` enums.
///
/// NOTE: No blanket impl - this is explicitly implemented by `CodeGen`.
pub trait KnownTypes: 'static + Clone + std::fmt::Debug {
    /// Downcast to concrete type via Any
    fn as_any(&self) -> &dyn Any;

    /// Get the BAML type name
    fn type_name(&self) -> &'static str;
}
