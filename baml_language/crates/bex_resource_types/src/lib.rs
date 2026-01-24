//! Resource types for external operations.
//!
//! This crate defines opaque resource handles that can be stored on the VM heap.
//! The actual resources (files, sockets) are managed by the sys provider.

use std::sync::Arc;

/// Type of resource for identification and cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    File,
    Socket,
}

/// Cleanup function type - called when handle is dropped.
type CleanupFn = Arc<dyn Fn(u64, ResourceType) + Send + Sync>;

/// An opaque handle to a resource managed by the sys provider.
///
/// The actual resource (Tokio file, socket, etc.) lives in the provider's registry.
/// When this handle is dropped, the cleanup callback notifies the provider.
pub struct ResourceHandle {
    /// Unique identifier for this resource.
    pub id: u64,
    /// Type of resource (for cleanup routing).
    pub kind: ResourceType,
    /// Path/address for display purposes.
    pub display_name: String,
    /// Cleanup callback - removes resource from provider's registry.
    cleanup: Option<CleanupFn>,
}

impl ResourceHandle {
    /// Create a new resource handle with a cleanup callback.
    pub fn new(
        id: u64,
        kind: ResourceType,
        display_name: String,
        cleanup: impl Fn(u64, ResourceType) + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            kind,
            display_name,
            cleanup: Some(Arc::new(cleanup)),
        }
    }

    /// Create a handle without cleanup (for testing or when cleanup is external).
    pub fn new_without_cleanup(id: u64, kind: ResourceType, display_name: String) -> Self {
        Self {
            id,
            kind,
            display_name,
            cleanup: None,
        }
    }
}

impl Clone for ResourceHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            kind: self.kind,
            display_name: self.display_name.clone(),
            cleanup: self.cleanup.clone(),
        }
    }
}

impl Drop for ResourceHandle {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            // Only call cleanup if this is the last reference
            if Arc::strong_count(&cleanup) == 1 {
                cleanup(self.id, self.kind);
            }
        }
    }
}

impl PartialEq for ResourceHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.kind == other.kind
    }
}

impl std::fmt::Debug for ResourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}(id={}, {})", self.kind, self.id, self.display_name)
    }
}

impl std::fmt::Display for ResourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            ResourceType::File => write!(f, "file:{}", self.display_name),
            ResourceType::Socket => write!(f, "socket:{}", self.display_name),
        }
    }
}
