//! Resource registry for external operations.
//!
//! Resources are Rust objects (file handles, connections, etc.) that external
//! operations can store and retrieve. The VM only sees integer resource IDs.
//!
//! This follows the same pattern as Deno's `ResourceTable`.

use std::{any::Any, collections::HashMap, sync::Arc};

/// A unique identifier for a resource.
pub type ResourceId = u64;

/// A resource that can be stored in the registry.
///
/// Any type that is `Send + Sync + 'static` can be stored as a resource.
/// The `Sized` bound ensures the blanket impl doesn't apply to `dyn Resource`.
pub trait Resource: Send + Sync + 'static {
    /// Downcast to a concrete type.
    fn as_any(&self) -> &dyn Any;
}

/// Blanket implementation for any sized type that satisfies the bounds.
/// The `Sized` bound is implicit but we make it explicit to prevent
/// the impl from applying to `dyn Resource` (which is `!Sized`).
impl<T: Send + Sync + 'static + Sized> Resource for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Registry for storing resources by ID.
///
/// External operations store resources here and return the ID to the VM.
/// Later operations can retrieve resources by ID.
#[derive(Default)]
pub struct ResourceRegistry {
    resources: HashMap<ResourceId, Arc<dyn Resource>>,
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
    pub fn add<T: Resource>(&mut self, resource: T) -> ResourceId {
        let id = self.next_id;
        self.next_id += 1;
        self.resources.insert(id, Arc::new(resource));
        id
    }

    /// Add an already-Arc'd resource and return its ID.
    pub fn add_arc(&mut self, resource: Arc<dyn Resource>) -> ResourceId {
        let id = self.next_id;
        self.next_id += 1;
        self.resources.insert(id, resource);
        id
    }

    /// Get a resource by ID, downcasting to the expected type.
    ///
    /// Returns None if the ID doesn't exist or the type doesn't match.
    pub fn get<T: 'static>(&self, id: ResourceId) -> Option<&T> {
        let resource = self.resources.get(&id)?;
        // Explicitly deref to call through the vtable, not the blanket impl
        // for Arc<dyn Resource> itself.
        (**resource).as_any().downcast_ref::<T>()
    }

    /// Get a cloned Arc of the resource by ID.
    pub fn get_arc(&self, id: ResourceId) -> Option<Arc<dyn Resource>> {
        self.resources.get(&id).cloned()
    }

    /// Get a resource by ID as a dynamic trait object.
    pub fn get_dyn(&self, id: ResourceId) -> Option<Arc<dyn Resource>> {
        self.resources.get(&id).cloned()
    }

    /// Remove a resource by ID.
    pub fn remove(&mut self, id: ResourceId) -> Option<Arc<dyn Resource>> {
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

    struct TestFile {
        path: String,
    }

    #[test]
    fn test_add_and_get() {
        let mut registry = ResourceRegistry::new();
        let file = TestFile {
            path: "/tmp/test.txt".to_string(),
        };
        let id = registry.add(file);

        let path = registry.get::<TestFile>(id).map(|f| f.path.clone());
        assert_eq!(path, Some("/tmp/test.txt".to_string()));
    }

    #[test]
    fn test_remove() {
        let mut registry = ResourceRegistry::new();
        let file = TestFile {
            path: "/tmp/test.txt".to_string(),
        };
        let id = registry.add(file);

        assert!(registry.contains(id));
        registry.remove(id);
        assert!(!registry.contains(id));
    }

    #[test]
    fn test_wrong_type() {
        let mut registry = ResourceRegistry::new();
        let file = TestFile {
            path: "/tmp/test.txt".to_string(),
        };
        let id = registry.add(file);

        // Try to get as wrong type
        let result = registry.get::<String>(id);
        assert!(result.is_none());
    }
}
