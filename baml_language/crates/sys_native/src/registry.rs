//! Resource registry for managing native Tokio resources.

#[cfg(feature = "bundle-http")]
use std::sync::{OnceLock, atomic::AtomicBool};
use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use bex_resource_types::{ResourceHandle, ResourceRegistryRef, ResourceType};
use sys_ops::io::VmBamlError;
use sys_types::sse::SseEvent;
use tokio::sync::Mutex as TokioMutex;
#[cfg(feature = "bundle-http")]
use tokio::{sync::Notify, task::AbortHandle};
#[cfg(feature = "bundle-http")]
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};

/// Buffer for SSE events accumulated by a background task.
pub struct SseBuffer {
    pub events: Vec<SseEvent>,
    pub done: bool,
    pub error: Option<VmBamlError>,
}

/// An SSE stream resource with buffered events.
#[cfg(feature = "bundle-http")]
pub struct SseStreamResource {
    pub buffer: Arc<TokioMutex<SseBuffer>>,
    pub closed: Arc<AtomicBool>,
    pub notify: Arc<Notify>,
    pub abort_handle: AbortHandle,
    pub url: String,
}

#[cfg(feature = "bundle-http")]
type SseStreamParts = (Arc<TokioMutex<SseBuffer>>, Arc<Notify>, Arc<AtomicBool>);

/// The write half of a connected WebSocket.
///
/// Type-erased rather than tied to one transport: a client socket
/// (`baml.ws.connect`) splits a plain-or-TLS TCP stream, while a server socket
/// (an HTTP upgrade inside `baml.http.Server.serve`) splits hyper's upgraded
/// IO. Both reach the registry — and therefore the whole `baml.ws.WebSocket`
/// surface — through this one type.
#[cfg(feature = "bundle-http")]
pub type WsSink = Box<dyn futures::Sink<WsMessage, Error = WsError> + Send + Unpin>;
/// The read half of a connected WebSocket. See [`WsSink`].
#[cfg(feature = "bundle-http")]
pub type WsSource = Box<dyn futures::Stream<Item = Result<WsMessage, WsError>> + Send + Unpin>;

/// How a WebSocket connection ended, as reported to BAML by
/// `baml.ws.WebSocket.next` (a `baml.ws.CloseEvent`).
#[cfg(feature = "bundle-http")]
#[derive(Clone, Debug)]
pub struct WsClose {
    /// RFC 6455 status code. `1005` ("no status received") when the peer's
    /// close frame carried no code and `1006` ("abnormal closure") when the
    /// stream ended with no closing handshake at all — the same two synthetic
    /// codes a browser reports on its `CloseEvent`.
    pub code: u16,
    pub reason: String,
}

/// A connected WebSocket.
///
/// Both transport halves are `Option` because the closing handshake releases
/// the socket eagerly: [`WsStreamResource::finish`] publishes `close` and drops
/// the halves together, so a terminated stream is never polled again and every
/// later `send`/`next` answers from `close` alone.
#[cfg(feature = "bundle-http")]
pub struct WsStreamResource {
    pub sink: TokioMutex<Option<WsSink>>,
    pub source: TokioMutex<Option<WsSource>>,
    pub close: OnceLock<WsClose>,
    pub url: String,
}

#[cfg(feature = "bundle-http")]
impl WsStreamResource {
    /// End the connection: complete the closing handshake, release the socket,
    /// and publish `close`.
    ///
    /// Closing the sink flushes the close frame tungstenite queues in reply to
    /// the peer's, which `read` alone leaves pending — without it the peer sees
    /// the connection vanish mid-handshake.
    ///
    /// The read half calls this the moment the connection ends, handing over
    /// its own `source` guard. Holding that guard across the whole call is what
    /// makes "the transport is gone" and "the close event is published"
    /// inseparable: no other caller can reach the source without the guard, and
    /// by then `close` is set.
    pub async fn finish(&self, source: &mut Option<WsSource>, close: WsClose) -> &WsClose {
        use futures::SinkExt;

        if let Some(mut sink) = self.sink.lock().await.take() {
            // Best effort — the peer may already be gone.
            let _ = sink.close().await;
        }
        *source = None;
        self.close.get_or_init(|| close)
    }

    /// Hang up: the side that owns this connection is done with it.
    ///
    /// Used by the HTTP server when a `WsAccept` handler returns, so a served
    /// connection's lifetime is the handler's rather than the garbage
    /// collector's.
    ///
    /// Takes the full teardown when nothing else is reading. If a `next` is in
    /// flight it closes only the write half and leaves that reader to publish
    /// the close event when the peer's echo (or EOF) arrives — waiting on the
    /// read lock here would block until a frame that may never come.
    pub async fn hangup(&self) {
        use futures::SinkExt;

        match self.source.try_lock() {
            Ok(mut source) => {
                if self.close.get().is_none() {
                    // `SinkExt::close` sends a status-less close frame, so
                    // `1005` is what both ends observe.
                    self.finish(
                        &mut source,
                        WsClose {
                            code: 1005,
                            reason: String::new(),
                        },
                    )
                    .await;
                }
            }
            Err(_) => {
                if let Some(mut sink) = self.sink.lock().await.take() {
                    let _ = sink.close().await;
                }
            }
        }
    }
}

/// Registry entry for a resource.
pub enum RegistryEntry {
    #[cfg(feature = "bundle-http")]
    SseStream(SseStreamResource),
    #[cfg(feature = "bundle-http")]
    WsStream(Arc<WsStreamResource>),
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

    #[cfg(feature = "bundle-http")]
    /// Register an SSE stream and return an opaque handle.
    pub fn register_sse_stream(
        self: &Arc<Self>,
        buffer: Arc<TokioMutex<SseBuffer>>,
        closed: Arc<AtomicBool>,
        notify: Arc<Notify>,
        abort_handle: AbortHandle,
        url: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let resource = SseStreamResource {
            buffer,
            closed,
            notify,
            abort_handle,
            url: url.clone(),
        };

        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, RegistryEntry::SseStream(resource));

        ResourceHandle::new(
            key,
            ResourceType::SseStream,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    #[cfg(feature = "bundle-http")]
    pub fn register_ws_stream(
        self: &Arc<Self>,
        sink: WsSink,
        source: WsSource,
        url: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                key,
                RegistryEntry::WsStream(Arc::new(WsStreamResource {
                    sink: TokioMutex::new(Some(sink)),
                    source: TokioMutex::new(Some(source)),
                    close: OnceLock::new(),
                    url: url.clone(),
                })),
            );

        ResourceHandle::new(
            key,
            ResourceType::WsStream,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    #[cfg(feature = "bundle-http")]
    pub fn get_ws_stream(&self, key: usize) -> Option<Arc<WsStreamResource>> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match entries.get(&key) {
            Some(RegistryEntry::WsStream(stream)) => Some(Arc::clone(stream)),
            _ => None,
        }
    }

    #[cfg(feature = "bundle-http")]
    /// Get the SSE stream buffer and notify handle.
    pub fn get_sse_stream(&self, key: usize) -> Option<SseStreamParts> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match entries.get(&key) {
            Some(RegistryEntry::SseStream(s)) => {
                Some((s.buffer.clone(), s.notify.clone(), s.closed.clone()))
            }
            _ => None,
        }
    }
}

impl ResourceRegistryRef for ResourceRegistry {
    fn remove(&self, key: usize) {
        let entry = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&key);

        #[cfg(feature = "bundle-http")]
        if let Some(RegistryEntry::SseStream(sse)) = entry {
            sse.closed.store(true, Ordering::Release);
            if let Ok(mut buf) = sse.buffer.try_lock() {
                buf.done = true;
                buf.error = None;
            }
            sse.abort_handle.abort();
            sse.notify.notify_waiters();
        }

        #[cfg(not(feature = "bundle-http"))]
        let _ = entry;
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
