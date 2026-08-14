//! Native HTTP/HTTPS server backing `baml.http.Server`.
//!
//! `Server.bind` opens the listener; `Server.serve` forwards to the `_serve`
//! sys-op implemented here. hyper owns the accept loop and the HTTP/1+2 protocol
//! (with `tokio_rustls` for HTTPS);
//! for every request, hyper's service invokes the BAML `handler` closure via
//! `VmSpawner::spawn_with_callable` — i.e. each request runs on its own BAML
//! thread. The closure reaches native code as a rooted [`Handle`]; it is never
//! serialized.
//!
//! Also hosts the `TlsConfig.new` / `Response.new` constructors and the unified
//! [`HttpBody`] used by both client (`fetch`/`send`) and server responses.

use std::{
    any::Any,
    convert::Infallible,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited, StreamBody, combinators::BoxBody};
use hyper::{
    Request as HyperRequest, Response as HyperResponse,
    body::{Frame, Incoming},
    header::{HeaderName, HeaderValue},
    service::service_fn,
};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use indexmap::IndexMap;
use sys_ops::io::{SysOpOutput, VmBamlError, owned};
use sys_types::{AsBexExternalValue, BexExternalValue, CancellationToken, Handle, VmSpawner};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer},
    },
};

// `timeout_from_nanos` lives in `io_impls` (always compiled) since the net
// sys-ops use it without the `bundle-http` feature; re-exported here for the
// HTTP server's own timeout fields.
use crate::io_impls::timeout_from_nanos;

/// The response body carried by `baml.http.Response._body`.
///
/// A client response (from `fetch`/`send`) holds a lazy `reqwest` body; a
/// server response built with `Response.new` holds buffered bytes. Keeping both
/// behind one `$rust_type` lets `Response.text()`/`bytes()` and the server's
/// response writer accept either — including proxying a fetched response
/// straight back out of a handler.
pub(crate) enum HttpBody {
    /// A streaming client response body, consumed at most once.
    Client(tokio::sync::Mutex<Option<reqwest::Response>>),
    /// A fully-buffered body.
    Bytes(Bytes),
    /// A server response body written incrementally by the handler via
    /// `Response.write` / `Response.end` (built by `Response.new_streaming`).
    Streaming(StreamingBody),
}

/// Backs an [`HttpBody::Streaming`] body. The `sender` side is driven by
/// `Response.write`/`end`; the `receiver` side is taken once by the response
/// writer ([`wire_response`]) and drained by hyper as chunked frames. The
/// channel is bounded at one in-flight chunk so `write` backpressures until the
/// previous chunk has been handed to the connection — giving real-time streaming
/// rather than a buffer that flushes all at once.
pub(crate) struct StreamingBody {
    sender: tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<Bytes>>>,
    receiver: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<Bytes>>>,
}

impl HttpBody {
    /// Wrap a freshly received client response.
    pub(crate) fn client(response: reqwest::Response) -> Arc<dyn Any + Send + Sync> {
        Arc::new(HttpBody::Client(tokio::sync::Mutex::new(Some(response))))
    }

    /// Consume the body and return it as bytes (reading the network if needed).
    pub(crate) async fn read_bytes(&self) -> Result<Bytes, VmBamlError> {
        match self {
            HttpBody::Bytes(b) => Ok(b.clone()),
            HttpBody::Client(slot) => {
                let resp = Self::take_client(slot)?;
                resp.bytes().await.map_err(|e| VmBamlError::Io {
                    message: format!("failed to read response body: {e}"),
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

    /// End a streaming body, closing the channel so hyper completes the chunked
    /// response. A no-op on an already-ended or non-streaming body.
    pub(crate) async fn end_stream(&self) -> Result<(), VmBamlError> {
        if let HttpBody::Streaming(s) = self {
            // Dropping the stored sender closes the channel once no `write` is
            // in flight (writes hold only transient clones), so the receiver
            // sees end-of-stream.
            s.sender.lock().await.take();
        }
        Ok(())
    }

    /// Consume the body and decode it as text. For a client response this honors
    /// the `Content-Type` charset (via `reqwest`); buffered bytes are UTF-8.
    pub(crate) async fn read_text(&self) -> Result<String, VmBamlError> {
        match self {
            // `text()` declares `throws Io`, so a decode failure maps to `Io`.
            HttpBody::Bytes(b) => String::from_utf8(b.to_vec()).map_err(|e| VmBamlError::Io {
                message: format!("Invalid UTF-8 in response body: {e}"),
            }),
            HttpBody::Client(slot) => {
                let resp = Self::take_client(slot)?;
                resp.text().await.map_err(|e| VmBamlError::Io {
                    message: format!("failed to read response body: {e}"),
                })
            }
            HttpBody::Streaming(_) => Err(VmBamlError::Io {
                message: "a streaming response body cannot be read with bytes()/text(); \
                          it is written with write()/end()"
                    .to_string(),
            }),
        }
    }

    fn take_client(
        slot: &tokio::sync::Mutex<Option<reqwest::Response>>,
    ) -> Result<reqwest::Response, VmBamlError> {
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
) -> Result<Arc<HttpBody>, VmBamlError> {
    body.clone()
        .downcast::<HttpBody>()
        .map_err(|_| VmBamlError::DevOther {
            message: "invalid HTTP response body handle".to_string(),
        })
}

/// The response body type written to the wire. Boxed so a handler's response
/// can be either fully buffered ([`Full`]) or incrementally streamed
/// ([`StreamBody`], for [`HttpBody::Streaming`]).
type WireBody = BoxBody<Bytes, Infallible>;

/// A plaintext or TLS connection, unified so hyper can serve either.
enum MaybeTlsStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::server::TlsStream<TcpStream>>),
}

impl AsyncRead for MaybeTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Build a `tokio_rustls` acceptor from a parsed `TlsConfig` and the server's
/// protocol flags (which drive ALPN advertisement).
fn build_acceptor(
    cfg: owned::http::TlsConfig,
    allow_http1: bool,
    allow_http2: bool,
) -> Result<TlsAcceptor, VmBamlError> {
    crate::ensure_rustls_crypto_provider();

    let certs = cfg
        ._certificate
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| VmBamlError::DevOther {
            message: "invalid TLS certificate handle".to_string(),
        })?;
    let key = cfg
        ._private_key
        .downcast::<PrivateKeyDer<'static>>()
        .map_err(|_| VmBamlError::DevOther {
            message: "invalid TLS private key handle".to_string(),
        })?;

    let tls13_only: [&'static rustls::SupportedProtocolVersion; 1] = [&rustls::version::TLS13];
    let versions: &[&rustls::SupportedProtocolVersion] = if cfg.allow_tls1_2 {
        rustls::ALL_VERSIONS
    } else {
        &tls13_only
    };

    let mut server_config = rustls::ServerConfig::builder_with_protocol_versions(versions)
        .with_no_client_auth()
        .with_single_cert(certs.as_ref().clone(), key.clone_key())
        .map_err(|e| VmBamlError::Io {
            message: format!("invalid TLS configuration: {e}"),
        })?;

    // Advertise only the protocols the server allows; the client picks one.
    let mut alpn: Vec<Vec<u8>> = Vec::new();
    if allow_http2 {
        alpn.push(b"h2".to_vec());
    }
    if allow_http1 {
        alpn.push(b"http/1.1".to_vec());
    }
    server_config.alpn_protocols = alpn;

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

/// Backing state for `baml.http.Server._state`: the bound listener plus a flag
/// enforcing one active `serve` at a time. The listener lives here (rather than
/// as a `serve` local) so cancelling a serve keeps the socket bound and the port
/// held, letting the same `Server` be served again. The port is released only
/// when the `Server` (and any in-flight serve) is dropped.
struct ServerState {
    listener: TcpListener,
    serving: AtomicBool,
}

fn downcast_server_state(server: &owned::http::Server) -> Result<Arc<ServerState>, VmBamlError> {
    server
        ._state
        .clone()
        .downcast::<ServerState>()
        .map_err(|_| VmBamlError::DevOther {
            message: "invalid HTTP server handle".to_string(),
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

/// Backs `Server.bind`: bind a TCP listener and return a `Server` carrying it.
/// The resolved local address (with the OS-assigned port for `":0"`) is stored
/// in `Server.addr`.
pub(crate) fn bind(addr: String) -> SysOpOutput<owned::http::Server> {
    SysOpOutput::async_op(async move {
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| VmBamlError::Io {
                message: format!("failed to bind '{addr}': {e}"),
            })?;
        let bound = listener.local_addr().map_err(|e| VmBamlError::Io {
            message: format!("failed to read bound address for '{addr}': {e}"),
        })?;
        let state: Arc<dyn Any + Send + Sync> = Arc::new(ServerState {
            listener,
            serving: AtomicBool::new(false),
        });
        Ok(owned::http::Server {
            addr: bound.to_string(),
            _state: state,
        })
    })
}

/// Backs `Server.serve`: run the accept loop on the server's already-bound
/// listener until cancelled. The returned future never resolves on its own; the
/// sys-op dispatcher drops it on cancellation (serve shutdown), which clears the
/// "serving" flag (so the `Server` can be served again) and — via the connection
/// `JoinSet`'s `Drop` — aborts every in-flight connection task. The listener
/// stays bound (it lives in the `Server`), so the port is retained for a restart.
#[expect(clippy::too_many_arguments)]
pub(crate) fn serve(
    server: owned::http::Server,
    handler: Handle,
    tls_config: Option<owned::http::TlsConfig>,
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
        // At least one slot; clamp to the semaphore's permit ceiling.
        let max_connections = usize::try_from(max_connections.max(1))
            .unwrap_or(usize::MAX)
            .min(Semaphore::MAX_PERMITS);
        let header_read_timeout = timeout_from_nanos(&header_read_timeout_nanos);

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

        // Each TLS connection inherits its handshake timeout from the config.
        let acceptor = match tls_config {
            Some(cfg) => {
                let handshake_timeout = timeout_from_nanos(&cfg._handshake_timeout_nanos);
                Some((
                    build_acceptor(cfg, allow_http1, allow_http2)?,
                    handshake_timeout,
                ))
            }
            None => None,
        };

        // Owns the in-flight connection tasks so they are aborted when this
        // future is dropped on cancellation (`JoinSet::Drop` aborts its tasks).
        let mut conns: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        // Caps concurrent connections; a permit is held for each connection's
        // lifetime and released when its task ends.
        let conn_limit = Arc::new(Semaphore::new(max_connections));
        let mut accept_backoff_ms: u64 = 0;
        loop {
            let accepted = tokio::select! {
                result = state.listener.accept() => result,
                // Reap finished connections so the set doesn't grow unbounded;
                // only polled when there is something to reap.
                Some(_) = conns.join_next(), if !conns.is_empty() => continue,
            };

            let stream = match accepted {
                Ok((stream, _peer)) => {
                    accept_backoff_ms = 0;
                    stream
                }
                Err(_) => {
                    // Transient accept errors (ECONNABORTED, or EMFILE/ENFILE under
                    // FD exhaustion) must never take the server down; back off
                    // (capped at 1s) instead of hot-spinning, then retry.
                    accept_backoff_ms = accept_backoff_ms.saturating_mul(2).clamp(1, 1000);
                    tokio::time::sleep(std::time::Duration::from_millis(accept_backoff_ms)).await;
                    continue;
                }
            };

            // Backpressure: wait for a free connection slot. At the cap the
            // accepted socket waits here (and the kernel backlog holds the rest)
            // rather than spawning past the limit.
            let permit = Arc::clone(&conn_limit)
                .acquire_owned()
                .await
                .expect("connection semaphore is never closed");
            // Reap finished connections so the JoinSet tracks only live ones.
            while conns.try_join_next().is_some() {}

            let acceptor = acceptor.clone();
            let handler = handler.clone();
            let spawner = Arc::clone(&spawner);
            let cancel = cancel.clone();
            conns.spawn(async move {
                // Held for the connection's lifetime; dropping it frees the slot.
                let _permit = permit;
                let stream = match acceptor {
                    Some((acceptor, handshake_timeout)) => {
                        let handshake = acceptor.accept(stream);
                        // A failed or timed-out TLS handshake closes just this connection.
                        let tls = match handshake_timeout {
                            Some(t) => match tokio::time::timeout(t, handshake).await {
                                Ok(Ok(tls)) => tls,
                                Ok(Err(_)) | Err(_) => return,
                            },
                            None => match handshake.await {
                                Ok(tls) => tls,
                                Err(_) => return,
                            },
                        };
                        MaybeTlsStream::Tls(Box::new(tls))
                    }
                    None => MaybeTlsStream::Plain(stream),
                };

                let io = TokioIo::new(stream);
                let service = service_fn(move |req: HyperRequest<Incoming>| {
                    let handler = handler.clone();
                    let spawner = Arc::clone(&spawner);
                    let cancel = cancel.child_token();
                    async move {
                        Ok::<_, Infallible>(
                            handle_request(req, handler, spawner, cancel, max_body_size).await,
                        )
                    }
                });

                serve_connection(io, service, allow_http1, allow_http2, header_read_timeout).await;
            });
        }
    })
}

/// Serve a single connection with the protocol(s) the server allows. Both
/// allowed → negotiate automatically (ALPN for TLS, prefix sniffing for
/// cleartext); otherwise pin to the single allowed protocol.
async fn serve_connection<S>(
    io: TokioIo<MaybeTlsStream>,
    service: S,
    allow_http1: bool,
    allow_http2: bool,
    header_read_timeout: Option<Duration>,
) where
    S: hyper::service::Service<
            HyperRequest<Incoming>,
            Response = HyperResponse<WireBody>,
            Error = Infallible,
        > + Send
        + 'static,
    S::Future: Send,
{
    if allow_http1 && allow_http2 {
        let mut builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
        if let Some(t) = header_read_timeout {
            // `header_read_timeout` needs a timer set, or it panics when it fires.
            builder
                .http1()
                .timer(TokioTimer::new())
                .header_read_timeout(t);
        }
        let _ = builder.serve_connection(io, service).await;
    } else if allow_http1 {
        let mut builder = hyper::server::conn::http1::Builder::new();
        if let Some(t) = header_read_timeout {
            builder.timer(TokioTimer::new()).header_read_timeout(t);
        }
        let _ = builder.serve_connection(io, service).await;
    } else if allow_http2 {
        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(io, service)
            .await;
    }
}

/// hyper service body: translate the request, run the BAML `handler` on its own
/// thread, and turn its `Response` into a wire response. Per-request failures
/// are isolated as 4xx/5xx — one bad request never stops the server.
async fn handle_request(
    req: HyperRequest<Incoming>,
    handler: Handle,
    spawner: Arc<dyn VmSpawner>,
    cancel: CancellationToken,
    max_body_size: usize,
) -> HyperResponse<WireBody> {
    let request = match to_baml_request(req, max_body_size).await {
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

/// Why a request couldn't be turned into a BAML `Request`.
enum BadRequest {
    /// The body exceeded `max_body_size` → 413.
    TooLarge,
    /// Malformed request / body read error → 400.
    Malformed,
}

/// Convert a hyper request into the BAML `Request` shape, capping the buffered
/// body at `max_body_size` bytes. The body is decoded lossily as UTF-8 to match
/// `Request.body: string`.
async fn to_baml_request(
    req: HyperRequest<Incoming>,
    max_body_size: usize,
) -> Result<owned::http::Request, BadRequest> {
    let (parts, body) = req.into_parts();
    let method = parts.method.as_str().to_string();
    let url = parts.uri.to_string();
    // `headers` is `map<string,string>`, but a header name may repeat. Fold
    // repeated values with ", " (RFC 7230 §3.2.2) — except `Cookie`, which folds
    // with "; " (RFC 6265 §5.4); HTTP/2 in particular may split it across fields.
    // Keeping every value means multiple `X-Forwarded-For` / `Via` aren't lost.
    let mut headers: IndexMap<String, String> = IndexMap::new();
    for (name, value) in &parts.headers {
        let value = String::from_utf8_lossy(value.as_bytes());
        let sep = if name == hyper::header::COOKIE {
            "; "
        } else {
            ", "
        };
        headers
            .entry(name.as_str().to_string())
            .and_modify(|existing| {
                existing.push_str(sep);
                existing.push_str(&value);
            })
            .or_insert_with(|| value.into_owned());
    }
    // `Limited` stops reading past the cap, so an oversized (or unbounded
    // chunked) body can't force unbounded allocation: a length overflow is a
    // 413, any other read error a 400.
    let body_bytes = match Limited::new(body, max_body_size).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) if e.downcast_ref::<LengthLimitError>().is_some() => {
            return Err(BadRequest::TooLarge);
        }
        Err(_) => return Err(BadRequest::Malformed),
    };
    let body = String::from_utf8_lossy(&body_bytes).into_owned();

    Ok(owned::http::Request {
        method,
        url,
        headers,
        body,
    })
}

/// Response headers a handler must not set, because hyper owns message framing
/// (`Content-Length` / `Transfer-Encoding` for the `Full<Bytes>` body) or because
/// they are hop-by-hop (RFC 9110 §7.6.1) and must not be forwarded — notably when
/// a handler proxies a fetched response straight back. `HeaderName::from_bytes`
/// lowercases the name, so a lowercase match is exhaustive.
fn is_reserved_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
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

/// Turn the handler's `Response` (a `BexExternalValue`) into a hyper response.
async fn wire_response(value: BexExternalValue) -> Result<HyperResponse<WireBody>, VmBamlError> {
    let response =
        owned::http::Response::from_external(value).map_err(|e| VmBamlError::DevOther {
            message: format!("handler returned an invalid Response: {e}"),
        })?;
    // A status outside the valid HTTP range (or u16) is a handler bug; fail
    // closed with 500 rather than silently serving 200.
    let status = u16::try_from(response.status_code)
        .ok()
        .filter(|s| (100..=599).contains(s))
        .unwrap_or(500);

    // A streaming response (`Response.new_streaming`) hands the body to hyper as
    // a frame stream drained from the `write`/`end` channel; everything else
    // is fully buffered. hyper frames a frame-stream body with chunked
    // transfer-encoding and flushes each frame, so partial writes reach the
    // client incrementally.
    let body_handle = downcast_body(&response._body)?;
    let wire_body = match &*body_handle {
        HttpBody::Streaming(s) => {
            let rx = s
                .receiver
                .lock()
                .await
                .take()
                .ok_or_else(|| VmBamlError::Io {
                    message: "streaming response body has already been served".to_string(),
                })?;
            let frames = futures::stream::unfold(rx, |mut rx| async move {
                rx.recv()
                    .await
                    .map(|chunk| (Ok::<_, Infallible>(Frame::data(chunk)), rx))
            });
            StreamBody::new(frames).boxed()
        }
        _ => Full::new(body_handle.read_bytes().await?).boxed(),
    };

    let mut builder = HyperResponse::builder().status(status);
    for (name, value) in &response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            // hyper owns framing/hop-by-hop headers (see is_reserved_response_header).
            if is_reserved_response_header(&name) {
                continue;
            }
            builder = builder.header(name, value);
        }
    }
    // A builder error (e.g. a malformed status that slipped through) → 500, not
    // a silent empty 200.
    Ok(match builder.body(wire_body) {
        Ok(resp) => resp,
        Err(_) => status_response(500, "invalid response"),
    })
}

/// A small plaintext response for internal/handler error conditions.
fn status_response(status: u16, message: &str) -> HyperResponse<WireBody> {
    HyperResponse::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::copy_from_slice(message.as_bytes())).boxed())
        .unwrap_or_else(|_| HyperResponse::new(Full::new(Bytes::new()).boxed()))
}

// ============================================================================
// Constructors called from the trait impls in `io_impls.rs`
// ============================================================================

/// Backs `TlsConfig.new`: parse a PEM cert chain + key into an opaque config.
pub(crate) fn tls_config_new(
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    allow_tls1_2: bool,
    handshake_timeout_nanos: Arc<num_bigint::BigInt>,
) -> SysOpOutput<owned::http::TlsConfig> {
    let mut cert_reader = std::io::Cursor::new(cert_pem);
    let certs = match rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>() {
        Ok(certs) if !certs.is_empty() => certs,
        Ok(_) => {
            return SysOpOutput::err(VmBamlError::Io {
                message: "no certificates found in cert_pem".to_string(),
            });
        }
        Err(e) => {
            return SysOpOutput::err(VmBamlError::Io {
                message: format!("invalid certificate PEM: {e}"),
            });
        }
    };

    let mut key_reader = std::io::Cursor::new(key_pem);
    let key = match rustls_pemfile::private_key(&mut key_reader) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return SysOpOutput::err(VmBamlError::Io {
                message: "no private key found in key_pem".to_string(),
            });
        }
        Err(e) => {
            return SysOpOutput::err(VmBamlError::Io {
                message: format!("invalid private key PEM: {e}"),
            });
        }
    };

    SysOpOutput::ok(owned::http::TlsConfig {
        allow_tls1_2,
        _certificate: Arc::new(certs) as Arc<dyn Any + Send + Sync>,
        _private_key: Arc::new(key) as Arc<dyn Any + Send + Sync>,
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
