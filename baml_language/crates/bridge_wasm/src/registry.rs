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
use sys_types::sse::SseParser;

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

/// Mutable state for an active SSE stream.
struct SseStreamInner {
    stream: Option<ByteStream>,
    parser: SseParser,
    done: bool,
}

/// Opaque handle stored in `owned::http::SseStream._handle`.
///
/// Contains the reqwest byte stream and SSE parser. The entire struct is
/// wrapped in `SendWrapper` so it satisfies `Send + Sync` on the
/// single-threaded WASM target.
pub(crate) struct WasmSseStreamHandle {
    /// Interior-mutable state. `SendWrapper` provides the required
    /// `Send + Sync` impls for WASM's single-threaded runtime.
    inner: SendWrapper<RefCell<SseStreamInner>>,
}

// SAFETY: wasm32-unknown-unknown is single-threaded.
unsafe impl Send for WasmSseStreamHandle {}
unsafe impl Sync for WasmSseStreamHandle {}

impl WasmSseStreamHandle {
    pub(crate) fn new(stream: ByteStream) -> Self {
        Self {
            inner: SendWrapper::new(RefCell::new(SseStreamInner {
                stream: Some(stream),
                parser: SseParser::new(),
                done: false,
            })),
        }
    }

    /// Take the byte stream and parser out for async use by `next()`.
    ///
    /// Returns `None` if already done or if the stream was already taken.
    pub(crate) fn take_stream(&self) -> Option<(ByteStream, SseParser)> {
        let mut inner = self.inner.borrow_mut();
        if inner.done {
            return None;
        }
        let stream = inner.stream.take()?;
        let parser = std::mem::replace(&mut inner.parser, SseParser::new());
        Some((stream, parser))
    }

    /// Return the byte stream and parser after `next()` is done.
    pub(crate) fn return_stream(&self, stream: ByteStream, parser: SseParser) {
        let mut inner = self.inner.borrow_mut();
        inner.stream = Some(stream);
        inner.parser = parser;
    }

    /// Mark the stream as done (no more events will be produced).
    pub(crate) fn mark_done(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.done = true;
        // Drop the stream to abort any in-flight fetch.
        inner.stream.take();
    }

    /// Returns true if the stream has been marked done.
    pub(crate) fn is_done(&self) -> bool {
        self.inner.borrow().done
    }
}
