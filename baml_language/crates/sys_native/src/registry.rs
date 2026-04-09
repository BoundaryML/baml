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
use sys_types::sse::SseEvent;
#[cfg(feature = "bundle-http")]
use tokio::task::AbortHandle;
use tokio::{
    fs::File,
    net::TcpStream,
    sync::{Mutex as TokioMutex, Notify},
};

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

/// The body of an HTTP response: either a real response or a synthetic error.
#[cfg(feature = "bundle-http")]
pub enum ResponseBody {
    /// A real HTTP response (None after body consumed).
    Real(Option<reqwest::Response>),
    /// A synthetic error response with the error message as body text.
    Error(Option<String>),
}

/// An HTTP response resource with lazy body consumption.
pub struct ResponseResource {
    #[cfg(feature = "bundle-http")]
    pub body: Arc<TokioMutex<ResponseBody>>,
}

/// Buffer for SSE events accumulated by a background task.
pub struct SseBuffer {
    pub events: Vec<SseEvent>,
    pub done: bool,
    pub error: Option<String>,
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

/// Registry entry for a resource.
pub enum RegistryEntry {
    File(FileResource),
    Socket(SocketResource),
    Response(ResponseResource),
    #[cfg(feature = "bundle-http")]
    SseStream(SseStreamResource),
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

    #[cfg(feature = "bundle-http")]
    /// Register an HTTP response and return an opaque handle.
    pub fn register_http_response(
        self: &Arc<Self>,
        response: reqwest::Response,
        url: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let resource = ResponseResource {
            body: Arc::new(TokioMutex::new(ResponseBody::Real(Some(response)))),
        };

        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::Response(resource));

        ResourceHandle::new(
            key,
            ResourceType::Response,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    #[cfg(feature = "bundle-http")]
    /// Register a synthetic error HTTP response with the error message as body.
    ///
    /// Used when a network error occurs, so BAML code can check `ok() == false`
    /// and optionally read the error via `text()`.
    pub fn register_error_http_response(
        self: &Arc<Self>,
        url: String,
        error_message: String,
    ) -> ResourceHandle {
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);
        let resource = ResponseResource {
            body: Arc::new(TokioMutex::new(ResponseBody::Error(Some(error_message)))),
        };

        self.entries
            .write()
            .unwrap()
            .insert(key, RegistryEntry::Response(resource));

        ResourceHandle::new(
            key,
            ResourceType::Response,
            url,
            Arc::clone(self) as Arc<dyn ResourceRegistryRef>,
        )
    }

    #[cfg(feature = "bundle-http")]
    /// Get the HTTP response mutex for body consumption.
    pub fn get_http_response_body(&self, key: usize) -> Option<Arc<TokioMutex<ResponseBody>>> {
        let entries = self.entries.read().unwrap();
        match entries.get(&key) {
            Some(RegistryEntry::Response(r)) => Some(r.body.clone()),
            _ => None,
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
