//! `baml.http.Server`, and the response bodies `baml.http` shares between
//! clients and servers, on the process's network transport (`baml_http`).
//!
//! `Server.bind` opens the listener; `Server.serve` forwards to the `_serve`
//! sys-op implemented here. The transport owns the accept loop, TLS and the
//! HTTP protocol; for every request it calls back here, and this module runs
//! the BAML `handler` closure via `VmSpawner::spawn_with_callable` — i.e. each
//! request runs on its own BAML thread. The closure reaches native code as a
//! rooted [`Handle`]; it is never serialized.
//!
//! An HTTP/1.1 WebSocket handshake (RFC 6455) is routed to the separate
//! `websocket` closure instead. That one returns either a `Response` refusing
//! the upgrade or a `WsAccept` callable; accepting hands the transport's
//! socket to the resource registry as a `baml.ws.WebSocket`, so a served
//! socket and a `baml.ws.connect` socket are the same BAML value.
//!
//! Also hosts the `TlsConfig.new` / `Response.new` constructors and the unified
//! [`HttpBody`] used by both client (`fetch`/`send`) and server responses.

use std::{
    any::{Any, TypeId},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use baml_http::{
    BodyStream, Bytes, CollectError, Headers, Listener, ResponseBody, ServeOptions, ServerRequest,
    ServerResponse, TlsConfig,
};
use bex_external_types::BexExternalAdt;
use indexmap::IndexMap;
use sys_ops::io::{SysOpOutput, VmBamlError, owned};
use sys_types::{
    AsBexExternalValue, BexExternalValue, CancellationToken, Handle, VmInternalError,
    VmRustFnError, VmSpawner,
};

// `timeout_from_nanos` lives in `io_impls` (always compiled) since the net
// sys-ops use it without the `bundle-http` feature; re-exported here for the
// HTTP server's own timeout fields.
use crate::io_impls::timeout_from_nanos;

/// The response body carried by `baml.http.Response._body`.
///
/// A client response (from `fetch`/`send`) holds the transport's lazy body; a
/// server response built with `Response.new` holds buffered bytes. Keeping both
/// behind one `$rust_type` lets `Response.text()`/`bytes()` and the server's
/// response writer accept either — including proxying a fetched response
/// straight back out of a handler.
pub(crate) enum HttpBody {
    /// A streaming client response body, consumed at most once.
    Client(tokio::sync::Mutex<Option<BodyStream>>),
    /// A fully-buffered body.
    Bytes(Bytes),
    /// A server response body written incrementally by the handler via
    /// `Response.write` / `Response.end` (built by `Response.new_streaming`).
    Streaming(StreamingBody),
}

/// Backs an [`HttpBody::Streaming`] body. The `sender` side is driven by
/// `Response.write`/`end`; the `receiver` side is taken once by the response
/// writer ([`wire_response`]) and drained by the transport. The channel is
/// bounded at one in-flight chunk so `write` backpressures until the previous
/// chunk has been handed to the connection — giving real-time streaming
/// rather than a buffer that flushes all at once.
pub(crate) struct StreamingBody {
    sender: tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<Bytes>>>,
    receiver: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<Bytes>>>,
}

impl HttpBody {
    /// Wrap a freshly received client response body.
    pub(crate) fn client(body: BodyStream) -> Arc<dyn Any + Send + Sync> {
        Arc::new(HttpBody::Client(tokio::sync::Mutex::new(Some(body))))
    }

    /// Consume the body and return it as bytes (reading the network if needed).
    pub(crate) async fn read_bytes(&self) -> Result<Bytes, VmBamlError> {
        match self {
            HttpBody::Bytes(b) => Ok(b.clone()),
            HttpBody::Client(slot) => {
                let body = Self::take_client(slot)?;
                baml_http::collect(body, usize::MAX)
                    .await
                    .map_err(|e| match e {
                        CollectError::Body(e) => crate::io_impls::http_transport_error(
                            "failed to read response body",
                            &e,
                        ),
                        CollectError::TooLarge => VmBamlError::Io {
                            message: "failed to read response body: too large".to_string(),
                        },
                    })
            }
            HttpBody::Streaming(_) => Err(VmBamlError::Io {
                message: "a streaming response body cannot be read with bytes()/text(); \
                          it is written with write()/end()"
                    .to_string(),
            }),
        }
    }

    /// Write one chunk to a streaming body, blocking (via the bounded channel)
    /// until the connection has accepted the previous chunk. Errors if this is
    /// not a streaming body, it has been ended, or the client hung up.
    pub(crate) async fn write_chunk(&self, data: Vec<u8>) -> Result<(), VmBamlError> {
        let HttpBody::Streaming(s) = self else {
            return Err(VmBamlError::Io {
                message: "Response.write requires a response built with Response.new_streaming"
                    .to_string(),
            });
        };
        // Clone the sender out so the `send().await` (which may suspend on
        // backpressure) does not hold the sender lock.
        let sender = s.sender.lock().await.clone();
        match sender {
            Some(tx) => tx
                .send(Bytes::from(data))
                .await
                .map_err(|_| VmBamlError::Io {
                    message: "streaming response could not be written: the client has hung up"
                        .to_string(),
                }),
            None => Err(VmBamlError::Io {
                message: "streaming response has already been ended".to_string(),
            }),
        }
    }

    /// End a streaming body, closing the channel so the transport completes the
    /// chunked response. A no-op on an already-ended or non-streaming body.
    pub(crate) async fn end_stream(&self) -> Result<(), VmBamlError> {
        if let HttpBody::Streaming(s) = self {
            // Dropping the stored sender closes the channel once no `write` is
            // in flight (writes hold only transient clones), so the receiver
            // sees end-of-stream.
            s.sender.lock().await.take();
        }
        Ok(())
    }

    /// Consume the body and decode it as text. Client responses decode lossily
    /// as UTF-8; buffered bytes require valid UTF-8.
    pub(crate) async fn read_text(&self) -> Result<String, VmBamlError> {
        match self {
            HttpBody::Bytes(b) => String::from_utf8(b.to_vec()).map_err(|e| VmBamlError::Io {
                message: format!("Invalid UTF-8 in response body: {e}"),
            }),
            HttpBody::Client(_) => {
                let bytes = self.read_bytes().await?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            HttpBody::Streaming(_) => Err(VmBamlError::Io {
                message: "a streaming response body cannot be read with bytes()/text(); \
                          it is written with write()/end()"
                    .to_string(),
            }),
        }
    }

    fn take_client(
        slot: &tokio::sync::Mutex<Option<BodyStream>>,
    ) -> Result<BodyStream, VmBamlError> {
        slot.try_lock()
            .ok()
            .and_then(|mut guard| guard.take())
            // Both callers (`text()`/`bytes()`) declare `throws root.errors.Io`,
            // so a double-consume surfaces as `Io` to stay catchable in-contract.
            .ok_or_else(|| VmBamlError::Io {
                message: "Response body has already been consumed".to_string(),
            })
    }
}

/// Downcast a `$rust_type` body field to [`HttpBody`].
pub(crate) fn downcast_body(
    body: &Arc<dyn Any + Send + Sync>,
) -> Result<Arc<HttpBody>, VmInternalError> {
    body.clone()
        .downcast::<HttpBody>()
        .map_err(|_| VmInternalError::RustTypeError {
            expected: TypeId::of::<HttpBody>(),
            got: body.type_id(),
        })
}

/// Backing state for `baml.http.Server._state`: the bound listener plus a flag
/// enforcing one active `serve` at a time. The listener lives here (rather than
/// as a `serve` local) so cancelling a serve keeps the socket bound and the port
/// held, letting the same `Server` be served again. The port is released only
/// when the `Server` (and any in-flight serve) is dropped.
struct ServerState {
    listener: Arc<dyn Listener>,
    serving: AtomicBool,
}

fn downcast_server_state(
    server: &owned::http::Server,
) -> Result<Arc<ServerState>, VmInternalError> {
    server
        ._state
        .clone()
        .downcast::<ServerState>()
        .map_err(|_| VmInternalError::RustTypeError {
            expected: TypeId::of::<ServerState>(),
            got: server._state.type_id(),
        })
}

/// Clears a [`ServerState`]'s "serving" flag when dropped, so cancelling a serve
/// (which drops its future) releases the slot for a future `serve` on the same
/// `Server`.
struct ServingGuard(Arc<ServerState>);

impl Drop for ServingGuard {
    fn drop(&mut self) {
        self.0.serving.store(false, Ordering::Release);
    }
}

/// A transport failure as a `baml.errors.Io`.
fn io_error(e: &baml_http::Error) -> VmBamlError {
    VmBamlError::Io {
        message: e.message.clone(),
    }
}

/// Backs `Server.bind`: bind a listener and return a `Server` carrying it.
/// The resolved local address (with the OS-assigned port for `":0"`) is stored
/// in `Server.addr`.
pub(crate) fn bind(addr: String) -> SysOpOutput<owned::http::Server> {
    SysOpOutput::async_op(async move {
        let listener = baml_http::provider()
            .bind(addr)
            .await
            .map_err(|e| io_error(&e))?;
        let addr = listener.local_addr();
        let state: Arc<dyn Any + Send + Sync> = Arc::new(ServerState {
            listener,
            serving: AtomicBool::new(false),
        });
        Ok(owned::http::Server {
            addr,
            _state: state,
        })
    })
}

/// A `TlsConfig`'s PEM, as the transport takes it.
fn tls_config(cfg: &owned::http::TlsConfig) -> Result<TlsConfig, VmInternalError> {
    let pem = |field: &Arc<dyn Any + Send + Sync>| {
        field
            .clone()
            .downcast::<Vec<u8>>()
            .map(|pem| pem.as_ref().clone())
            .map_err(|field| VmInternalError::RustTypeError {
                expected: TypeId::of::<Vec<u8>>(),
                got: field.type_id(),
            })
    };
    Ok(TlsConfig {
        cert_pem: pem(&cfg._certificate)?,
        key_pem: pem(&cfg._private_key)?,
        allow_tls1_2: cfg.allow_tls1_2,
        handshake_timeout: timeout_from_nanos(&cfg._handshake_timeout_nanos),
    })
}

/// Backs `Server.serve`: serve the server's already-bound listener until
/// cancelled. The returned future never resolves on its own; the sys-op
/// dispatcher drops it on cancellation (serve shutdown), which clears the
/// "serving" flag (so the `Server` can be served again) and ends every
/// in-flight connection. The listener stays bound (it lives in the `Server`),
/// so the port is retained for a restart.
#[expect(clippy::too_many_arguments)]
pub(crate) fn serve(
    server: owned::http::Server,
    handler: Handle,
    websocket: Handle,
    tls_config_value: Option<owned::http::TlsConfig>,
    allow_http1: bool,
    allow_http2: bool,
    max_body_size: i64,
    max_connections: i64,
    header_read_timeout_nanos: Arc<num_bigint::BigInt>,
    spawner: Arc<dyn VmSpawner>,
    cancel: CancellationToken,
) -> SysOpOutput<()> {
    SysOpOutput::async_op(async move {
        if !allow_http1 && !allow_http2 {
            return Err(VmBamlError::Io {
                message: "server must allow at least one of HTTP/1 or HTTP/2".to_string(),
            }
            .into());
        }

        let state = downcast_server_state(&server)?;
        // Negative → 0 (reject any body); too-large-for-usize (32-bit) → uncapped.
        let max_body_size = usize::try_from(max_body_size.max(0)).unwrap_or(usize::MAX);
        // At least one slot.
        let max_connections = usize::try_from(max_connections.max(1)).unwrap_or(usize::MAX);
        let tls = tls_config_value.as_ref().map(tls_config).transpose()?;

        // One active serve per `Server`. The CAS claims the slot; the guard
        // releases it when this future ends — including a cancellation-drop — so
        // the same `Server` can be served again afterward.
        if state
            .serving
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(VmBamlError::Io {
                message: "server is already serving".to_string(),
            }
            .into());
        }
        let _serving_guard = ServingGuard(Arc::clone(&state));

        let options = ServeOptions {
            tls,
            allow_http1,
            allow_http2,
            max_connections,
            header_read_timeout: timeout_from_nanos(&header_read_timeout_nanos),
        };
        let request_handler: baml_http::Handler = Arc::new(move |request: ServerRequest| {
            let handler = handler.clone();
            let websocket = websocket.clone();
            let spawner = Arc::clone(&spawner);
            let cancel = cancel.clone();
            Box::pin(async move {
                if request.websocket_upgrade {
                    handle_websocket(request, websocket, spawner, cancel).await
                } else {
                    handle_request(
                        request,
                        handler,
                        spawner,
                        cancel.child_token(),
                        max_body_size,
                    )
                    .await
                }
            })
        });

        Arc::clone(&state.listener)
            .serve(options, request_handler)
            .await
            .map_err(|e| io_error(&e))?;
        Ok(())
    })
}

/// Run the BAML `handler` for one request on its own thread, and turn its
/// `Response` into the transport's. Per-request failures are isolated as
/// 4xx/5xx — one bad request never stops the server.
async fn handle_request(
    request: ServerRequest,
    handler: Handle,
    spawner: Arc<dyn VmSpawner>,
    cancel: CancellationToken,
    max_body_size: usize,
) -> ServerResponse {
    let request = match to_baml_request(request, max_body_size).await {
        Ok(request) => request,
        Err(BadRequest::TooLarge) => return status_response(413, "payload too large"),
        Err(BadRequest::Malformed) => return status_response(400, "bad request"),
    };

    match spawner
        .spawn_with_callable(handler, vec![request.into_bex_external_value()], cancel)
        .await
    {
        Ok(response) => match wire_response(response).await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!("HTTP handler returned an invalid response: {e}");
                status_response(500, "handler returned an invalid response")
            }
        },
        // The handler threw, panicked, or was cancelled mid-flight. The spawner
        // error is opaque (`Box<dyn Send + Sync>`) here, and serve-shutdown
        // cancels every in-flight request at once, so log at `debug` without it
        // rather than flooding `warn`.
        Err(_) => {
            tracing::debug!("HTTP request handler failed (threw, panicked, or was cancelled)");
            status_response(500, "request handler failed")
        }
    }
}

// ============================================================================
// WebSocket upgrades
// ============================================================================

/// What a `websocket` handler decided.
#[expect(
    clippy::large_enum_variant,
    reason = "One short-lived local per handshake, moved straight into the \
              matching branch; boxing would add an allocation to the refuse \
              path to shrink a value that is never stored."
)]
enum WsOutcome {
    /// Complete the handshake and run this `WsAccept` with the socket.
    Accept(Handle),
    /// Refuse the upgrade and serve this `Response` instead.
    Refuse(BexExternalValue),
}

/// Classify a `websocket` handler's `Response | WsAccept` return.
///
/// The two arms are told apart by *shape*, not by the declared type: a callable
/// crosses as a rooted `TaggedHeapHandle` — the same handle
/// `spawn_with_callable` consumes, so the accept callback can be invoked later
/// without re-entering the heap — while a `Response` crosses as a plain
/// `Instance`.
///
/// Deliberately not keyed on the handle's `ty`: closures do not participate in
/// the engine's union discrimination, so a returned `WsAccept` is tagged with
/// whichever union member happens to come first (here `Response`). The only
/// other value the engine tags this way is an `ai.stream.Stream` instance,
/// which this union cannot hold.
///
/// Anything that is not a tagged handle is the `Response` arm and is validated
/// by [`wire_response`], which reports a malformed one as a 500 rather than
/// guessing.
fn websocket_outcome(value: BexExternalValue) -> WsOutcome {
    // A union-typed return arrives wrapped in its union metadata.
    let value = match value {
        BexExternalValue::Union { value, .. } => *value,
        value => value,
    };
    match value {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) => {
            WsOutcome::Accept(heap_handle)
        }
        value => WsOutcome::Refuse(value),
    }
}

/// Serve a WebSocket upgrade: run the BAML `websocket` handler, then either
/// refuse with the `Response` it returned or accept, handing the connected
/// socket to its `WsAccept`.
async fn handle_websocket(
    request: ServerRequest,
    websocket: Handle,
    spawner: Arc<dyn VmSpawner>,
    cancel: CancellationToken,
) -> ServerResponse {
    // A handshake carries no body, so the handler sees the usual `Request`
    // shape with an empty body.
    let request = owned::http::Request {
        method: request.method,
        url: request.url,
        headers: collect_headers(&request.headers),
        body: String::new(),
    };
    let url = request.url.clone();

    // Same isolation as an ordinary request: one failed handshake never stops
    // the server. See `handle_request` for why this logs at `debug`.
    let Ok(outcome) = Arc::clone(&spawner)
        .spawn_with_callable(
            websocket,
            vec![request.into_bex_external_value()],
            cancel.child_token(),
        )
        .await
    else {
        tracing::debug!("WebSocket handler failed (threw, panicked, or was cancelled)");
        return status_response(500, "websocket handler failed");
    };

    let accept = match websocket_outcome(outcome) {
        WsOutcome::Accept(accept) => accept,
        WsOutcome::Refuse(response) => {
            return match wire_response(response).await {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!("WebSocket handler returned an invalid response: {e}");
                    status_response(500, "websocket handler returned an invalid response")
                }
            };
        }
    };

    let handler_cancel = cancel.child_token();
    ServerResponse::AcceptWebSocket(Box::new(move |socket| {
        Box::pin(async move {
            // The socket outlives the request, so it is bounded by the serve's
            // cancellation — without this a shut-down server would leave its
            // sockets running.
            tokio::select! {
                () = cancel.cancelled() => {}
                () = run_websocket(socket, url, accept, spawner, handler_cancel) => {}
            }
        })
    }))
}

/// Run the BAML `WsAccept` handler with the connected socket, which it owns:
/// the connection is hung up once it returns.
async fn run_websocket(
    socket: baml_http::WebSocket,
    url: String,
    accept: Handle,
    spawner: Arc<dyn VmSpawner>,
    cancel: CancellationToken,
) {
    // Holding `handle` past the call keeps the registry entry alive for the
    // handler's whole run; dropping it at the end of this function is what
    // finally releases the socket.
    let handle = crate::registry::REGISTRY.register_ws_stream(socket.sink, socket.source, url);
    let socket = owned::ws::WebSocket {
        _handle: Arc::new(handle.clone()),
    };

    let _ = spawner
        .spawn_with_callable(accept, vec![socket.into_bex_external_value()], cancel)
        .await;

    // The handler is done with the connection whether it returned or failed.
    if let Some(resource) = crate::registry::REGISTRY.get_ws_stream(handle.key()) {
        resource.hangup().await;
    }
}

/// Why a request couldn't be turned into a BAML `Request`.
enum BadRequest {
    /// The body exceeded `max_body_size` → 413.
    TooLarge,
    /// Malformed request / body read error → 400.
    Malformed,
}

/// Convert a transport request into the BAML `Request` shape, capping the
/// buffered body at `max_body_size` bytes. The body is decoded lossily as
/// UTF-8 to match `Request.body: string`.
async fn to_baml_request(
    request: ServerRequest,
    max_body_size: usize,
) -> Result<owned::http::Request, BadRequest> {
    let headers = collect_headers(&request.headers);
    // Reading stops past the cap, so an oversized (or unbounded chunked) body
    // can't force unbounded allocation: a length overflow is a 413, any other
    // read error a 400.
    let body_bytes = match baml_http::collect(request.body, max_body_size).await {
        Ok(bytes) => bytes,
        Err(CollectError::TooLarge) => return Err(BadRequest::TooLarge),
        Err(CollectError::Body(_)) => return Err(BadRequest::Malformed),
    };
    Ok(owned::http::Request {
        method: request.method,
        url: request.url,
        headers,
        body: String::from_utf8_lossy(&body_bytes).into_owned(),
    })
}

/// Fold request headers into the `map<string, string>` shape of
/// `Request.headers`.
///
/// A header name may repeat, so repeated values are joined with ", " (RFC 7230
/// §3.2.2) — except `Cookie`, which joins with "; " (RFC 6265 §5.4); HTTP/2 in
/// particular may split it across fields. Keeping every value means multiple
/// `X-Forwarded-For` / `Via` aren't lost.
fn collect_headers(headers: &Headers) -> IndexMap<String, String> {
    let mut collected: IndexMap<String, String> = IndexMap::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        let sep = if name == "cookie" { "; " } else { ", " };
        collected
            .entry(name)
            .and_modify(|existing| {
                existing.push_str(sep);
                existing.push_str(value);
            })
            .or_insert_with(|| value.clone());
    }
    collected
}

/// Response headers a handler must not set, because the transport owns
/// message framing (`Content-Length` / `Transfer-Encoding`) or because they are
/// hop-by-hop (RFC 9110 §7.6.1) and must not be forwarded — notably when a
/// handler proxies a fetched response straight back.
fn is_reserved_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "upgrade"
            | "te"
            | "trailer"
            | "proxy-authenticate"
            | "proxy-authorization"
    )
}

/// Turn the handler's `Response` (a `BexExternalValue`) into the transport's.
async fn wire_response(value: BexExternalValue) -> Result<ServerResponse, VmRustFnError> {
    // The handler's return type is `baml.http.Response`, so a value that does
    // not decode as one is an engine/bridge inconsistency, not a handler error.
    let response = owned::http::Response::from_external(value).map_err(|e| {
        VmInternalError::BridgeFailure {
            message: format!("HTTP handler return value did not decode as a Response: {e}"),
        }
    })?;
    // A status outside the valid HTTP range (or u16) is a handler bug; fail
    // closed with 500 rather than silently serving 200.
    let status = u16::try_from(response.status_code)
        .ok()
        .filter(|s| (100..=599).contains(s))
        .unwrap_or(500);

    // A streaming response (`Response.new_streaming`) hands the body over as a
    // chunk stream drained from the `write`/`end` channel; everything else is
    // fully buffered.
    let body_handle = downcast_body(&response._body)?;
    let body = match &*body_handle {
        HttpBody::Streaming(s) => {
            let rx = s
                .receiver
                .lock()
                .await
                .take()
                .ok_or_else(|| VmBamlError::Io {
                    message: "streaming response body has already been served".to_string(),
                })?;
            let chunks = futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|chunk| (chunk, rx))
            });
            ResponseBody::Stream(Box::pin(chunks))
        }
        _ => ResponseBody::Full(body_handle.read_bytes().await?),
    };

    let headers = response
        .headers
        .iter()
        .filter(|(name, _)| !is_reserved_response_header(name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    Ok(ServerResponse::Http {
        status,
        headers,
        body,
    })
}

/// A small plaintext response for internal/handler error conditions.
fn status_response(status: u16, message: &str) -> ServerResponse {
    ServerResponse::Http {
        status,
        headers: vec![(
            "content-type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        )],
        body: ResponseBody::Full(Bytes::copy_from_slice(message.as_bytes())),
    }
}

// ============================================================================
// Constructors called from the trait impls in `io_impls.rs`
// ============================================================================

/// Backs `TlsConfig.new`: check a PEM cert chain + key with the transport and
/// keep the PEM for `serve`.
pub(crate) fn tls_config_new(
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    allow_tls1_2: bool,
    handshake_timeout_nanos: Arc<num_bigint::BigInt>,
) -> SysOpOutput<owned::http::TlsConfig> {
    let check = TlsConfig {
        cert_pem,
        key_pem,
        allow_tls1_2,
        handshake_timeout: timeout_from_nanos(&handshake_timeout_nanos),
    };
    if let Err(e) = baml_http::provider().check_tls(&check) {
        return SysOpOutput::err(io_error(&e));
    }
    SysOpOutput::ok(owned::http::TlsConfig {
        allow_tls1_2,
        _certificate: Arc::new(check.cert_pem) as Arc<dyn Any + Send + Sync>,
        _private_key: Arc::new(check.key_pem) as Arc<dyn Any + Send + Sync>,
        _handshake_timeout_nanos: handshake_timeout_nanos,
    })
}

/// Build a buffered `Response` from a handler, used by `Response.new`.
pub(crate) fn build_response(
    status_code: i64,
    headers: IndexMap<String, String>,
    body: Vec<u8>,
) -> owned::http::Response {
    owned::http::Response {
        status_code,
        headers,
        url: String::new(),
        _body: Arc::new(HttpBody::Bytes(Bytes::from(body))) as Arc<dyn Any + Send + Sync>,
    }
}

/// Build a streaming `Response`, used by `Response.new_streaming`. The body is
/// produced later by `Response.write`/`end` and drained by the wire writer
/// over a one-deep channel (see [`StreamingBody`]).
pub(crate) fn build_streaming_response(
    status_code: i64,
    headers: IndexMap<String, String>,
) -> owned::http::Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
    owned::http::Response {
        status_code,
        headers,
        url: String::new(),
        _body: Arc::new(HttpBody::Streaming(StreamingBody {
            sender: tokio::sync::Mutex::new(Some(tx)),
            receiver: tokio::sync::Mutex::new(Some(rx)),
        })) as Arc<dyn Any + Send + Sync>,
    }
}
