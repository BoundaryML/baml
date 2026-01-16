//! BEX Sys - System operations for the BEX runtime.
//!
//! This crate provides external I/O operations (file system, network, shell)
//! that the BEX engine can dispatch to. It is independent of the engine itself.
//!
//! # Architecture
//!
//! Operations receive an `OpContext` for resource management and `ResolvedArgs`
//! containing the operation arguments. They return `ResolvedValue` or `OpError`.
//!
//! Resources (file handles, sockets, etc.) are stored in a `ResourceRegistry`.
//! Operations can store resources and return their ID. Later operations can
//! retrieve resources by ID.

mod resource;

pub mod ops;

use std::{future::Future, pin::Pin, sync::Mutex};

pub use resource::{FileHandle, ResourceId, ResourceKind, ResourceRegistry, SocketHandle};

// ============================================================================
// Resolved Values
// ============================================================================

/// A resolved value that external operations can work with directly.
///
/// Unlike VM `Value` which may contain object indices, `ResolvedValue` contains
/// the actual data that external operations need.
#[derive(Debug, Clone)]
pub enum ResolvedValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<ResolvedValue>),
    Map(indexmap::IndexMap<String, ResolvedValue>),
    /// A resource ID (for file handles, connections, etc.)
    ResourceId(ResourceId),
}

// ============================================================================
// Operation Context and Errors
// ============================================================================

/// Errors that can occur during external operation execution.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("{0}")]
    Other(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(ResourceId),

    #[error("Resource type mismatch")]
    ResourceTypeMismatch,
}

/// Context passed to external operations.
///
/// Provides access to resources and other engine state.
pub struct OpContext {
    /// Registry for storing/retrieving resources.
    pub resources: Mutex<ResourceRegistry>,
}

impl OpContext {
    /// Create a new context with an empty resource registry.
    pub fn new() -> Self {
        Self {
            resources: Mutex::new(ResourceRegistry::new()),
        }
    }

    /// Add a resource and return its ID.
    pub fn add_resource(&self, resource: impl Into<ResourceKind>) -> ResourceId {
        self.resources.lock().unwrap().add(resource)
    }

    /// Check if a resource exists.
    pub fn has_resource(&self, id: ResourceId) -> bool {
        self.resources.lock().unwrap().contains(id)
    }

    /// Remove a resource by ID.
    pub fn remove_resource(&self, id: ResourceId) -> Option<ResourceKind> {
        self.resources.lock().unwrap().remove(id)
    }
}

impl Default for OpContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolved arguments for an external operation.
#[derive(Debug, Clone)]
pub struct ResolvedArgs {
    /// Resolved arguments.
    pub args: Vec<ResolvedValue>,
}

// ============================================================================
// Operation Results
// ============================================================================

/// A boxed future for async operations.
pub type OpFuture = Pin<Box<dyn Future<Output = Result<ResolvedValue, OpError>> + Send>>;

/// Result of a system operation - either immediate or async.
///
/// This allows sync operations (like `close`) to complete immediately without
/// spawning a task, while async operations (like `open`, `read`) return futures.
pub enum SysOpResult {
    /// Operation completed synchronously with this result.
    Ready(Result<ResolvedValue, OpError>),
    /// Operation is async and needs to be awaited.
    Async(OpFuture),
}
