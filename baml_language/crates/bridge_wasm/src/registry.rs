//! Resource registry for WASM - stores HTTP response body promises.
//!
//! In `sys_native`, response bodies live in a registry as `reqwest::Response`
//! and are consumed lazily. For WASM, the JS fetch callback returns a
//! `bodyPromise` (`Promise<string>`); we store that and await it only when
//! `response.text()` is called.

use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use js_sys::Promise;

use crate::send_wrapper::SendWrapper;

/// An HTTP response stored in the WASM registry.
struct ResponseEntry {
    /// Promise that resolves to the response body text (awaited when `.text()` is called).
    /// Wrapped for Send+Sync on WASM (single-threaded).
    body_promise: Option<SendWrapper<Promise>>,
}

/// Registry entry for a resource.
enum RegistryEntry {
    Response(ResponseEntry),
}

/// WASM resource registry.
///
/// Stores HTTP response body promises and provides opaque keys.
/// When a [`WasmResponseBody`] is dropped, it automatically removes the entry.
pub(crate) struct WasmRegistry {
    next_key: AtomicUsize,
    entries: RwLock<HashMap<usize, RegistryEntry>>,
}

impl WasmRegistry {
    /// Create a new empty registry.
    pub(crate) fn new() -> Self {
        Self {
            next_key: AtomicUsize::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Store an HTTP response body promise; returns the key for later retrieval.
    pub(crate) fn store_body_promise(&self, body_promise: Promise) -> usize {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let entry = ResponseEntry {
            body_promise: Some(SendWrapper::new(body_promise)),
        };
        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::Response(entry));
        key
    }

    /// Take the body promise for the given key.
    ///
    /// Keeps the entry so that the handle's Drop can remove it (Drop-driven cleanup).
    /// Returns `None` if the handle is invalid or body was already consumed.
    pub(crate) fn take_body_promise(&self, key: usize) -> Option<Promise> {
        let mut entries = self.entries.write().unwrap();
        match entries.get_mut(&key) {
            Some(RegistryEntry::Response(r)) => r
                .body_promise
                .take()
                .map(super::send_wrapper::SendWrapper::into_inner),
            _ => None,
        }
    }

    /// Remove an entry by key (called from [`WasmResponseBody::drop`]).
    pub(crate) fn remove(&self, key: usize) {
        self.entries.write().unwrap().remove(&key);
    }
}

/// Opaque body handle stored in `owned::http::Response._body` as `Arc<dyn Any + Send + Sync>`.
///
/// Ties the response's body promise to the registry and cleans up on drop.
pub(crate) struct WasmResponseBody {
    pub(crate) registry: Arc<WasmRegistry>,
    pub(crate) key: usize,
}

impl Drop for WasmResponseBody {
    fn drop(&mut self) {
        self.registry.remove(self.key);
    }
}
