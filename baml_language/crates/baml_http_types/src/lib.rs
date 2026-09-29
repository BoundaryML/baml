//! The network transport BAML runs on, as an interface.
//!
//! Every network connection BAML makes goes through one [`HttpProvider`]:
//! the HTTP requests a BAML program makes (LLM calls, `baml.http`), its
//! WebSocket connections (`baml.ws`) and servers (`baml.http.Server`), and
//! the requests BAML makes on its own (telemetry, login, self-update, the
//! remote cache). The default `baml_http_provider` crate implements it with
//! reqwest, hyper, rustls and tungstenite; a build can replace that crate
//! with its own implementation.
//!
//! The interface is the transport only. What BAML does on top of it (SSE
//! parsing, request-body limits, running a BAML handler per request, the
//! `baml.ws.WebSocket` resource) stays in BAML, so an implementation only has
//! to move bytes.

use std::{fmt, future::Future, pin::Pin, sync::Arc, time::Duration};

pub use bytes::Bytes;
use futures::{Sink, Stream, StreamExt};

/// A boxed, sendable future.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// A streamed message body.
pub type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, Error>> + Send + 'static>>;

/// Header name/value pairs, in wire order. Names may repeat.
pub type Headers = Vec<(String, String)>;

// ============================================================================
// Errors
// ============================================================================

/// What kind of failure an [`Error`] reports. BAML maps [`ErrorKind::Timeout`]
/// to `baml.errors.Timeout` and everything else to `baml.errors.Io`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A deadline elapsed (the request timeout, the connect timeout, ...).
    Timeout,
    /// The connection failed, or the peer broke the protocol.
    Io,
    /// The request itself was malformed (bad URL, header or method).
    InvalidRequest,
    /// The transport does not support the operation.
    Unsupported,
}

/// A transport failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Io, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidRequest, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }

    pub fn is_timeout(&self) -> bool {
        self.kind == ErrorKind::Timeout
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

// ============================================================================
// Client requests
// ============================================================================

/// Whether a client follows HTTP redirects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Redirects {
    /// Follow redirects (up to the implementation's limit).
    #[default]
    Follow,
    /// Return the 3xx response as is.
    None,
}

/// An HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    /// The method, e.g. `GET`. Validated by BAML before it reaches a provider.
    pub method: String,
    pub url: String,
    pub headers: Headers,
    /// Empty for no body.
    pub body: Bytes,
    /// Deadline for the whole exchange: connecting, the response head and the
    /// body. When it elapses mid-body, the body stream yields a
    /// [`ErrorKind::Timeout`] error.
    pub timeout: Option<Duration>,
    /// Deadline for establishing the connection alone.
    pub connect_timeout: Option<Duration>,
    pub redirects: Redirects,
    /// Whether to route through the proxy the environment configures
    /// (`HTTPS_PROXY` and friends).
    pub use_env_proxy: bool,
}

impl Request {
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: Vec::new(),
            body: Bytes::new(),
            timeout: None,
            connect_timeout: None,
            redirects: Redirects::Follow,
            use_env_proxy: true,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new("GET", url)
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::new("POST", url)
    }

    pub fn put(url: impl Into<String>) -> Self {
        Self::new("PUT", url)
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn redirects(mut self, redirects: Redirects) -> Self {
        self.redirects = redirects;
        self
    }

    #[must_use]
    pub fn use_env_proxy(mut self, use_env_proxy: bool) -> Self {
        self.use_env_proxy = use_env_proxy;
        self
    }
}

/// An HTTP response, with its body still streaming.
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    /// The URL that answered, after any redirects.
    pub url: String,
    pub body: BodyStream,
}

impl Response {
    /// The first value of header `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Read the whole body.
    pub async fn bytes(self) -> Result<Bytes, Error> {
        collect(self.body, usize::MAX).await.map_err(|e| match e {
            CollectError::Body(e) => e,
            CollectError::TooLarge => Error::io("response body is too large"),
        })
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

/// Why [`collect`] stopped.
#[derive(Debug)]
pub enum CollectError {
    /// The body is longer than the limit.
    TooLarge,
    /// The body stream failed.
    Body(Error),
}

/// Read a body into memory, failing once it exceeds `limit` bytes.
pub async fn collect(mut body: BodyStream, limit: usize) -> Result<Bytes, CollectError> {
    let mut out = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(CollectError::Body)?;
        if out.len().saturating_add(chunk.len()) > limit {
            return Err(CollectError::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(out))
}

// ============================================================================
// WebSockets
// ============================================================================

/// A WebSocket data or close message. Implementations answer pings
/// themselves; they never reach BAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    /// A close frame, with the peer's code and reason when it sent one.
    Close(Option<CloseFrame>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

/// The write half of a WebSocket.
pub type WsSink = Pin<Box<dyn Sink<Message, Error = Error> + Send + 'static>>;
/// The read half of a WebSocket. Ends when the connection does.
pub type WsSource = Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send + 'static>>;

/// A connected WebSocket.
pub struct WebSocket {
    pub sink: WsSink,
    pub source: WsSource,
}

/// Opening a client WebSocket (`ws://` or `wss://`).
#[derive(Debug, Clone)]
pub struct WsConnectRequest {
    pub url: String,
    /// Extra handshake headers.
    pub headers: Headers,
}

// ============================================================================
// Servers
// ============================================================================

/// TLS for a server, as PEM.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// The certificate chain.
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    /// Whether TLS 1.2 is accepted alongside 1.3.
    pub allow_tls1_2: bool,
    /// Deadline for each connection's TLS handshake.
    pub handshake_timeout: Option<Duration>,
}

/// How a [`Listener`] serves.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub tls: Option<TlsConfig>,
    pub allow_http1: bool,
    pub allow_http2: bool,
    /// The most connections open at once, counting WebSocket connections accepted from
    /// them. Further connections wait for a free slot.
    pub max_connections: usize,
    /// Deadline for reading an HTTP/1 request head.
    pub header_read_timeout: Option<Duration>,
}

/// A request a server received.
pub struct ServerRequest {
    pub method: String,
    /// The request target, e.g. `/path?query`.
    pub url: String,
    pub headers: Headers,
    /// Empty for a WebSocket handshake.
    pub body: BodyStream,
    /// Whether this is an RFC 6455 WebSocket handshake. The handler answers
    /// it with [`ServerResponse::AcceptWebSocket`] to accept, or any HTTP
    /// response to refuse.
    pub websocket_upgrade: bool,
}

/// A response body a server writes.
pub enum ResponseBody {
    Full(Bytes),
    /// Written as each chunk arrives (chunked transfer on HTTP/1).
    Stream(Pin<Box<dyn Stream<Item = Bytes> + Send + 'static>>),
}

/// Runs an accepted server WebSocket. The connection is closed when the
/// returned future finishes, and it keeps counting against
/// [`ServeOptions::max_connections`] until then.
pub type OnWebSocket = Box<dyn FnOnce(WebSocket) -> BoxFuture<()> + Send + 'static>;

/// A server's answer to a [`ServerRequest`].
pub enum ServerResponse {
    Http {
        status: u16,
        headers: Headers,
        body: ResponseBody,
    },
    /// Complete a WebSocket handshake and hand the socket to the callback.
    /// Only valid for a request with `websocket_upgrade` set; the server
    /// answers any other request with a 500 instead.
    AcceptWebSocket(OnWebSocket),
}

/// Handles each request a server receives, each on its own task.
pub type Handler = Arc<dyn Fn(ServerRequest) -> BoxFuture<ServerResponse> + Send + Sync + 'static>;

/// A bound server socket.
pub trait Listener: Send + Sync + 'static {
    /// The bound address, with the port the OS assigned for port 0.
    fn local_addr(&self) -> String;

    /// Accept and serve connections until the returned future is dropped,
    /// which also ends every connection it opened. The socket stays bound, so
    /// the listener can serve again.
    fn serve(
        self: Arc<Self>,
        options: ServeOptions,
        handler: Handler,
    ) -> BoxFuture<Result<(), Error>>;
}

// ============================================================================
// The provider
// ============================================================================

/// The transport behind every network connection BAML makes.
pub trait HttpProvider: Send + Sync + 'static {
    /// Send an HTTP request.
    fn send(&self, request: Request) -> BoxFuture<Result<Response, Error>>;

    /// Open a client WebSocket.
    fn connect_websocket(&self, request: WsConnectRequest) -> BoxFuture<Result<WebSocket, Error>>;

    /// Bind a server socket, e.g. `127.0.0.1:0`.
    fn bind(&self, addr: String) -> BoxFuture<Result<Arc<dyn Listener>, Error>>;

    /// Check a server TLS configuration when `baml.http.TlsConfig` is built,
    /// so a bad certificate fails there rather than when serving starts.
    fn check_tls(&self, tls: &TlsConfig) -> Result<(), Error>;
}
