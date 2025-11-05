//! Core types and traits used throughout the BAML compiler.
//!
//! This crate has NO dependencies on other compiler crates to avoid circular dependencies.

pub mod core_types;
pub mod files;

// Re-export everything for convenience
pub use core_types::*;
pub use files::*;
