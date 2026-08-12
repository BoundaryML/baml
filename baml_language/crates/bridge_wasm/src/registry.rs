//! Resource registry for WASM - stores HTTP response body promises and SSE streams.
//!
//! In `sys_native`, response bodies live in a registry as `reqwest::Response`
//! and are consumed lazily. For WASM, the JS fetch callback returns a
//! `bodyPromise` (`Promise<string>`); we store that and await it only when
//! `response.text()` is called.
//!
//! SSE streams use reqwest's WASM byte-streaming support with `SseParser`
//! to parse events entirely in Rust, matching the native implementation.
#![allow(unsafe_code)]

use std::{
    cell::RefCell,
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures::stream::Stream;
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
/// Stores HTTP response body promises, providing opaque keys.
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

// ---------------------------------------------------------------------------
// SSE stream handle
// ---------------------------------------------------------------------------

/// A boxed byte stream from reqwest's `bytes_stream()`.
///
/// On WASM, this wraps a browser `ReadableStream` via reqwest's WASM backend.
pub(crate) type ByteStream = Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>>>>;

/// Channel receiver type for SSE events.
pub(crate) type SseEventReceiver =
    futures::channel::mpsc::UnboundedReceiver<Result<sys_types::sse::SseEvent, String>>;

/// Opaque handle stored in `owned::http::SseStream._handle`.
///
/// A background task (spawned via `wasm_bindgen_futures::spawn_local`) reads
/// from the byte stream, parses SSE events, and sends them through an mpsc
/// channel. `next()` drains available events from the receiver.
///
/// Dropping the receiver (via `close()` / `mark_done()`) signals the
/// background task to exit on its next send attempt.
pub(crate) struct WasmSseStreamHandle {
    /// Channel receiver for parsed SSE events. `None` after `close()`.
    receiver: SendWrapper<RefCell<Option<SseEventReceiver>>>,
}

// SAFETY: wasm32-unknown-unknown is single-threaded.
unsafe impl Send for WasmSseStreamHandle {}
unsafe impl Sync for WasmSseStreamHandle {}

impl WasmSseStreamHandle {
    pub(crate) fn new(receiver: SseEventReceiver) -> Self {
        Self {
            receiver: SendWrapper::new(RefCell::new(Some(receiver))),
        }
    }

    /// Returns true if the stream has been closed (receiver dropped).
    pub(crate) fn is_done(&self) -> bool {
        self.receiver.borrow().is_none()
    }

    /// Close the stream by dropping the receiver.
    ///
    /// The background task will detect the closed channel on its next send
    /// and exit.
    pub(crate) fn mark_done(&self) {
        self.receiver.borrow_mut().take();
    }

    /// Borrow the inner `RefCell` for synchronous drain and poll operations.
    pub(crate) fn receiver_ref(&self) -> &RefCell<Option<SseEventReceiver>> {
        self.receiver.inner()
    }
}
