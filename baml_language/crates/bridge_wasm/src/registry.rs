//! Resource registry for WASM - stores HTTP response bodies.
//!
//! In `sys_native`, response bodies live in a registry as `reqwest::Response`
//! and are consumed lazily. For WASM, the JS side returns the full body eagerly.
//! We store it in a simple HashMap keyed by handle ID.

use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use bex_resource_types::{ResourceHandle, ResourceRegistryRef, ResourceType};

/// An HTTP response stored in the WASM registry.
struct ResponseEntry {
    /// The response body text (already consumed from JS).
    body: Option<String>,
}

/// Registry entry for a resource.
enum RegistryEntry {
    Response(ResponseEntry),
}

/// WASM resource registry.
///
/// Stores HTTP response bodies and provides opaque handles.
/// When a handle is dropped, it automatically removes the entry.
pub(crate) struct WasmRegistry {
    next_key: AtomicUsize,
    entries: RwLock<HashMap<usize, RegistryEntry>>,
}

impl WasmRegistry {
    /// Create a new empty registry.
    fn new() -> Self {
        Self {
            next_key: AtomicUsize::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Register an HTTP response body and return an opaque handle.
    pub(crate) fn register_http_response(
        self: &Arc<Self>,
        body: String,
        url: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let entry = ResponseEntry { body: Some(body) };

        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::Response(entry));

        ResourceHandle::new(
            key,
            ResourceType::Response,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    /// Get and consume the HTTP response body.
    ///
    /// Returns `None` if the handle is invalid or body was already consumed.
    pub(crate) fn consume_http_response_body(&self, key: usize) -> Option<String> {
        let mut entries = self.entries.write().unwrap();
        match entries.get_mut(&key) {
            Some(RegistryEntry::Response(r)) => r.body.take(),
            _ => None,
        }
    }
}

impl ResourceRegistryRef for WasmRegistry {
    fn remove(&self, key: usize) {
        self.entries.write().unwrap().remove(&key);
    }
}

/// Global WASM resource registry instance.
pub(crate) static REGISTRY: std::sync::LazyLock<Arc<WasmRegistry>> =
    std::sync::LazyLock::new(|| Arc::new(WasmRegistry::new()));
