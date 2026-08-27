//! Resource registry for managing native Tokio resources.

#[cfg(feature = "bundle-http")]
use std::sync::atomic::AtomicBool;
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

#[cfg(feature = "bundle-http")]
type WsTransport =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
#[cfg(feature = "bundle-http")]
type WsSink = futures::stream::SplitSink<WsTransport, tokio_tungstenite::tungstenite::Message>;
#[cfg(feature = "bundle-http")]
type WsSource = futures::stream::SplitStream<WsTransport>;

#[cfg(feature = "bundle-http")]
pub struct WsStreamResource {
    pub sink: Arc<TokioMutex<WsSink>>,
    pub source: Arc<TokioMutex<WsSource>>,
    pub url: String,
}

#[cfg(feature = "bundle-http")]
type WsStreamParts = (Arc<TokioMutex<WsSink>>, Arc<TokioMutex<WsSource>>);

/// Registry entry for a resource.
pub enum RegistryEntry {
    #[cfg(feature = "bundle-http")]
    SseStream(SseStreamResource),
    #[cfg(feature = "bundle-http")]
    WsStream(WsStreamResource),
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
        sink: Arc<TokioMutex<WsSink>>,
        source: Arc<TokioMutex<WsSource>>,
        url: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                key,
                RegistryEntry::WsStream(WsStreamResource {
                    sink,
                    source,
                    url: url.clone(),
                }),
            );

        ResourceHandle::new(
            key,
            ResourceType::WsStream,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    #[cfg(feature = "bundle-http")]
    pub fn get_ws_stream(&self, key: usize) -> Option<WsStreamParts> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match entries.get(&key) {
            Some(RegistryEntry::WsStream(stream)) => {
                Some((stream.sink.clone(), stream.source.clone()))
            }
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
