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
    Response,
    SseStream,
    StreamAccumulator,
}

/// Trait for releasing resource handles back to the registry.
///
/// This is implemented by `ResourceRegistry` to allow handles to clean up
/// when dropped, without creating a circular dependency.
pub trait ResourceRegistryRef: Send + Sync {
    /// Remove a resource from the registry by its key.
    fn remove(&self, key: usize);
}

/// An opaque handle to a resource managed by the sys provider.
///
/// The actual resource (Tokio file, socket, etc.) lives in the provider's registry.
/// When the last clone of this handle is dropped, the resource is automatically
/// removed from the registry.
#[derive(Clone)]
pub struct ResourceHandle {
    inner: Arc<ResourceHandleInner>,
}

/// Internal handle data.
struct ResourceHandleInner {
    /// Key in the registry's `HashMap`.
    key: usize,
    /// Type of resource (for identification).
    kind: ResourceType,
    /// Path/address for display purposes.
    display_name: String,
    /// Reference to registry for cleanup on drop.
    registry: Option<Arc<dyn ResourceRegistryRef>>,
}

impl ResourceHandle {
    /// Create a new resource handle with automatic cleanup.
    pub fn new(
        key: usize,
        kind: ResourceType,
        display_name: String,
        registry: Arc<dyn ResourceRegistryRef>,
    ) -> Self {
        Self {
            inner: Arc::new(ResourceHandleInner {
                key,
                kind,
                display_name,
                registry: Some(registry),
            }),
        }
    }

    /// Create a handle without cleanup (for testing or when cleanup is external).
    pub fn new_without_cleanup(key: usize, kind: ResourceType, display_name: String) -> Self {
        Self {
            inner: Arc::new(ResourceHandleInner {
                key,
                kind,
                display_name,
                registry: None,
            }),
        }
    }

    /// Get the registry key for this handle.
    pub fn key(&self) -> usize {
        self.inner.key
    }

    /// Get the type of resource.
    pub fn kind(&self) -> ResourceType {
        self.inner.kind
    }

    /// Get the display name (path or address).
    pub fn display_name(&self) -> &str {
        &self.inner.display_name
    }
}

impl Drop for ResourceHandleInner {
    fn drop(&mut self) {
        // When the last ResourceHandle clone is dropped, remove from registry
        if let Some(ref registry) = self.registry {
            registry.remove(self.key);
        }
    }
}

impl PartialEq for ResourceHandle {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key() && self.kind() == other.kind()
    }
}

impl std::fmt::Debug for ResourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}(key={}, {})",
            self.kind(),
            self.key(),
            self.display_name()
        )
    }
}

impl std::fmt::Display for ResourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind() {
            ResourceType::File => write!(f, "file:{}", self.display_name()),
            ResourceType::Socket => write!(f, "socket:{}", self.display_name()),
            ResourceType::Response => write!(f, "http-response:{}", self.display_name()),
            ResourceType::SseStream => write!(f, "sse-stream:{}", self.display_name()),
            ResourceType::StreamAccumulator => {
                write!(f, "stream-accumulator:{}", self.display_name())
            }
        }
    }
}
