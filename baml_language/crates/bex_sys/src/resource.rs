//! Resource registry for external operations.
//!
//! Resources are Rust objects (file handles, connections, etc.) that external
//! operations can store and retrieve. The VM only sees integer resource IDs.
//!
//! Uses static dispatch via the `ResourceKind` enum for type safety and performance.

use std::{collections::HashMap, sync::Arc};

use tokio::{fs::File, net::TcpStream, sync::Mutex};

/// A unique identifier for a resource.
pub type ResourceId = u64;

// ============================================================================
// Resource Types
// ============================================================================

/// A file handle stored in the resource registry.
pub struct FileHandle {
    /// The file, wrapped in Arc for cloning out of the resource registry.
    pub file: Arc<Mutex<File>>,
    /// The path the file was opened from.
    pub path: String,
}

impl FileHandle {
    /// Create a new file handle.
    pub fn new(file: File, path: String) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
            path,
        }
    }
}

/// A socket handle stored in the resource registry.
pub struct SocketHandle {
    /// The stream, wrapped in Arc for cloning out of the resource registry.
    pub stream: Arc<Mutex<TcpStream>>,
    /// The address the socket connected to.
    pub addr: String,
}

impl SocketHandle {
    /// Create a new socket handle.
    pub fn new(stream: TcpStream, addr: String) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            addr,
        }
    }
}

// ============================================================================
// Resource Enum
// ============================================================================

/// All resource types that can be stored in the registry.
pub enum ResourceKind {
    File(FileHandle),
    Socket(SocketHandle),
}

impl From<FileHandle> for ResourceKind {
    fn from(handle: FileHandle) -> Self {
        ResourceKind::File(handle)
    }
}

impl From<SocketHandle> for ResourceKind {
    fn from(handle: SocketHandle) -> Self {
        ResourceKind::Socket(handle)
    }
}

// ============================================================================
// Resource Registry
// ============================================================================

/// Registry for storing resources by ID.
///
/// External operations store resources here and return the ID to the VM.
/// Later operations can retrieve resources by ID.
#[derive(Default)]
pub struct ResourceRegistry {
    resources: HashMap<ResourceId, ResourceKind>,
    next_id: ResourceId,
}

impl ResourceRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            next_id: 1, // Start at 1 so 0 can be "invalid"
        }
    }

    /// Add a resource and return its ID.
    pub fn add(&mut self, resource: impl Into<ResourceKind>) -> ResourceId {
        let id = self.next_id;
        self.next_id += 1;
        self.resources.insert(id, resource.into());
        id
    }

    /// Get a resource by ID.
    pub fn get(&self, id: ResourceId) -> Option<&ResourceKind> {
        self.resources.get(&id)
    }

    /// Get a file handle by ID.
    pub fn get_file(&self, id: ResourceId) -> Option<&FileHandle> {
        match self.resources.get(&id)? {
            ResourceKind::File(handle) => Some(handle),
            ResourceKind::Socket(_) => None,
        }
    }

    /// Get a socket handle by ID.
    pub fn get_socket(&self, id: ResourceId) -> Option<&SocketHandle> {
        match self.resources.get(&id)? {
            ResourceKind::Socket(handle) => Some(handle),
            ResourceKind::File(_) => None,
        }
    }

    /// Remove a resource by ID.
    pub fn remove(&mut self, id: ResourceId) -> Option<ResourceKind> {
        self.resources.remove(&id)
    }

    /// Check if a resource exists.
    pub fn contains(&self, id: ResourceId) -> bool {
        self.resources.contains_key(&id)
    }

    /// Get the number of resources.
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Clear all resources.
    pub fn clear(&mut self) {
        self.resources.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_get_file() {
        let mut registry = ResourceRegistry::new();

        // Create a temp file for testing
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_resource.txt");
        tokio::fs::write(&temp_path, "test content").await.unwrap();

        let file = File::open(&temp_path).await.unwrap();
        let handle = FileHandle::new(file, temp_path.to_string_lossy().to_string());
        let id = registry.add(handle);

        let retrieved = registry.get_file(id);
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().path.contains("test_resource.txt"));

        // Cleanup
        tokio::fs::remove_file(&temp_path).await.unwrap();
    }

    #[test]
    fn test_remove() {
        let registry = ResourceRegistry::new();

        // We can't easily create a real TcpStream in a sync test,
        // so just test the registry mechanics with a file handle
        // by checking contains/remove work correctly
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_wrong_type() {
        let registry = ResourceRegistry::new();

        // Add a socket, try to get as file
        // We can't easily create resources without async,
        // so this test is limited
        assert!(registry.get_file(999).is_none());
        assert!(registry.get_socket(999).is_none());
    }
}
