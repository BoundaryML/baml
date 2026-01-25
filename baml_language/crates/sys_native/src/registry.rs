//! Resource registry for managing native Tokio resources.

use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use sys_resource_types::{ResourceHandle, ResourceRegistryRef, ResourceType};
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
/// When a handle is dropped, it automatically removes the entry via `ResourceRegistryRef`.
pub struct ResourceRegistry {
    next_key: AtomicUsize,
    entries: RwLock<HashMap<usize, RegistryEntry>>,
}

impl ResourceRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            next_key: AtomicUsize::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register a file and return an opaque handle.
    pub fn register_file(self: &Arc<Self>, file: File, path: String) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let resource = FileResource {
            file: Arc::new(TokioMutex::new(file)),
            path: path.clone(),
        };

        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::File(resource));

        ResourceHandle::new(
            key,
            ResourceType::File,
            path,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    /// Register a socket and return an opaque handle.
    pub fn register_socket(self: &Arc<Self>, stream: TcpStream, addr: String) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let resource = SocketResource {
            stream: Arc::new(TokioMutex::new(stream)),
            addr: addr.clone(),
        };

        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::Socket(resource));

        ResourceHandle::new(
            key,
            ResourceType::Socket,
            addr,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    /// Get a file resource by handle key.
    pub fn get_file(&self, key: usize) -> Option<Arc<TokioMutex<File>>> {
        let entries = self.entries.read().unwrap();
        match entries.get(&key) {
            Some(RegistryEntry::File(f)) => Some(f.file.clone()),
            _ => None,
        }
    }

    /// Get a socket resource by handle key.
    pub fn get_socket(&self, key: usize) -> Option<Arc<TokioMutex<TcpStream>>> {
        let entries = self.entries.read().unwrap();
        match entries.get(&key) {
            Some(RegistryEntry::Socket(s)) => Some(s.stream.clone()),
            _ => None,
        }
    }
}

impl ResourceRegistryRef for ResourceRegistry {
    fn remove(&self, key: usize) {
        self.entries.write().unwrap().remove(&key);
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
