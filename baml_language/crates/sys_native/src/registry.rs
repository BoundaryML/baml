//! Resource registry for managing native Tokio resources.

use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_resource_types::{ResourceHandle, ResourceType};
use tokio::{fs::File, net::TcpStream, sync::Mutex as TokioMutex};

/// A file resource with async-safe access.
pub struct FileResource {
    pub file: Arc<TokioMutex<File>>,
    pub path: String,
}

/// A socket resource with async-safe access.
pub struct SocketResource {
    pub stream: Arc<TokioMutex<TcpStream>>,
    pub addr: String,
}

/// Registry entry for a resource.
pub enum RegistryEntry {
    File(FileResource),
    Socket(SocketResource),
}

/// Global resource registry.
///
/// Stores actual Tokio resources and provides opaque handles.
/// When a handle is dropped, its cleanup callback removes the entry.
pub struct ResourceRegistry {
    next_id: AtomicU64,
    entries: RwLock<HashMap<u64, RegistryEntry>>,
}

impl ResourceRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register a file and return an opaque handle.
    pub fn register_file(&self, file: File, path: String) -> ResourceHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let resource = FileResource {
            file: Arc::new(TokioMutex::new(file)),
            path: path.clone(),
        };

        self.entries
            .write()
            .unwrap()
            .insert(id, RegistryEntry::File(resource));

        ResourceHandle::new(id, ResourceType::File, path, {
            let registry = REGISTRY.clone();
            move |id, _kind| {
                registry.remove(id);
            }
        })
    }

    /// Register a socket and return an opaque handle.
    pub fn register_socket(&self, stream: TcpStream, addr: String) -> ResourceHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let resource = SocketResource {
            stream: Arc::new(TokioMutex::new(stream)),
            addr: addr.clone(),
        };

        self.entries
            .write()
            .unwrap()
            .insert(id, RegistryEntry::Socket(resource));

        ResourceHandle::new(id, ResourceType::Socket, addr, {
            let registry = REGISTRY.clone();
            move |id, _kind| {
                registry.remove(id);
            }
        })
    }

    /// Get a file resource by handle ID.
    pub fn get_file(&self, id: u64) -> Option<Arc<TokioMutex<File>>> {
        let entries = self.entries.read().unwrap();
        match entries.get(&id) {
            Some(RegistryEntry::File(f)) => Some(f.file.clone()),
            _ => None,
        }
    }

    /// Get a socket resource by handle ID.
    pub fn get_socket(&self, id: u64) -> Option<Arc<TokioMutex<TcpStream>>> {
        let entries = self.entries.read().unwrap();
        match entries.get(&id) {
            Some(RegistryEntry::Socket(s)) => Some(s.stream.clone()),
            _ => None,
        }
    }

    /// Remove a resource from the registry.
    pub fn remove(&self, id: u64) {
        self.entries.write().unwrap().remove(&id);
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global resource registry instance.
pub static REGISTRY: std::sync::LazyLock<Arc<ResourceRegistry>> =
    std::sync::LazyLock::new(|| Arc::new(ResourceRegistry::new()));
