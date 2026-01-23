//! BEX Sys - System operations for the BEX runtime.
//!
//! This crate provides external I/O operations (file system, network, shell)
//! that the BEX engine can dispatch to. Operations receive and return
//! `BexExternalValue` directly.

pub mod ops;

use std::{future::Future, pin::Pin};

// Re-export BexExternalValue for ops
pub use bex_external_types::BexExternalValue;
// Re-export resource types
pub use bex_resource_types::{FileHandle, ResourceKind, SocketHandle};

// ============================================================================
// Operation Errors
// ============================================================================

/// Errors that can occur during external operation execution.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("{0}")]
    Other(String),

    #[error("Expected {expected}, got {actual}")]
    TypeError {
        expected: &'static str,
        actual: String,
    },

    #[error("Expected resource of type {expected}")]
    ResourceTypeMismatch { expected: &'static str },
}

// ============================================================================
// Operation Results
// ============================================================================

/// A boxed future for async operations.
pub type OpFuture = Pin<Box<dyn Future<Output = Result<BexExternalValue, OpError>> + Send>>;

/// Result of a system operation - either immediate or async.
pub enum SysOpResult {
    /// Operation completed synchronously with this result.
    Ready(Result<BexExternalValue, OpError>),
    /// Operation is async and needs to be awaited.
    Async(OpFuture),
}
