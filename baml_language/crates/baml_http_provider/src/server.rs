//! HTTP/HTTPS servers, on hyper and tokio-rustls.
//!
//! hyper owns the accept loop and the HTTP/1+2 protocol (with `tokio_rustls`
//! for HTTPS). Each request goes to the [`Handler`] on its own task. An
//! HTTP/1.1 WebSocket handshake (RFC 6455) reaches the handler flagged as one;
//! accepting it hands hyper's upgraded connection to tungstenite.

use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use baml_http_types::{
    BoxFuture, Bytes, Error, Handler, Headers, Listener, ResponseBody, ServeOptions, ServerRequest,
    ServerResponse, TlsConfig,
};
use futures::StreamExt;
use http_body_util::{BodyExt, BodyStream, Empty, Full, StreamBody, combinators::UnsyncBoxBody};
use hyper::{
    Request as HyperRequest, Response as HyperResponse,
    body::{Frame, Incoming},
    header::{
        CONNECTION, HeaderMap, HeaderName, HeaderValue, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY,
        SEC_WEBSOCKET_VERSION, UPGRADE,
    },
    service::service_fn,
};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{handshake::derive_accept_key, protocol::Role},
};

/// The response body type written to the wire: fully buffered ([`Full`]) or
/// streamed ([`StreamBody`]).
type WireBody = UnsyncBoxBody<Bytes, Infallible>;

/// A connection's `max_connections` slot, shared with the request service so a
/// WebSocket upgrade can take it over from the HTTP connection that carried the
/// handshake. Empty once taken, or once the connection has ended.
type ConnPermit = Arc<std::sync::Mutex<Option<tokio::sync::OwnedSemaphorePermit>>>;

// ============================================================================
// TLS
// ============================================================================

fn parse_certs(tls: &TlsConfig) -> Result<Vec<CertificateDer<'static>>, Error> {
    let certs = CertificateDer::pem_slice_iter(&tls.cert_pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::io(format!("invalid certificate PEM: {e}")))?;
    if certs.is_empty() {
        return Err(Error::io("no certificates found in cert_pem"));
    }
    Ok(certs)
}

fn parse_key(tls: &TlsConfig) -> Result<PrivateKeyDer<'static>, Error> {
    PrivateKeyDer::from_pem_slice(&tls.key_pem).map_err(|e| match e {
        rustls::pki_types::pem::Error::NoItemsFound => Error::io("no private key found in key_pem"),
        e => Error::io(format!("invalid private key PEM: {e}")),
    })
}

pub(crate) fn check_tls(tls: &TlsConfig) -> Result<(), Error> {
    parse_certs(tls)?;
    parse_key(tls)?;
    Ok(())
}

/// A `tokio_rustls` acceptor for `tls`, advertising (ALPN) only the protocols
/// the server allows.
fn build_acceptor(
    tls: &TlsConfig,
    allow_http1: bool,
    allow_http2: bool,
) -> Result<TlsAcceptor, Error> {
    crate::ensure_crypto_provider()?;
    let certs = parse_certs(tls)?;
    let key = parse_key(tls)?;

    let tls13_only: [&'static rustls::SupportedProtocolVersion; 1] = [&rustls::version::TLS13];
    let versions: &[&rustls::SupportedProtocolVersion] = if tls.allow_tls1_2 {
        rustls::ALL_VERSIONS
    } else {
        &tls13_only
    };
    let mut config = ServerConfig::builder_with_protocol_versions(versions)
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| Error::io(format!("invalid TLS configuration: {e}")))?;

    let mut alpn: Vec<Vec<u8>> = Vec::new();
    if allow_http2 {
        alpn.push(b"h2".to_vec());
    }
    if allow_http1 {
        alpn.push(b"http/1.1".to_vec());
    }
    config.alpn_protocols = alpn;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

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

// ============================================================================
// Listener
// ============================================================================

pub(crate) fn bind(addr: String) -> BoxFuture<Result<Arc<dyn Listener>, Error>> {
    Box::pin(async move {
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| Error::io(format!("failed to bind '{addr}': {e}")))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| Error::io(format!("failed to read bound address for '{addr}': {e}")))?;
        Ok(Arc::new(NativeListener {
            listener,
            local_addr: local_addr.to_string(),
        }) as Arc<dyn Listener>)
    })
}

/// A bound TCP listener. It lives as long as the `Listener`, not a serve, so
/// cancelling a serve keeps the port.
struct NativeListener {
    listener: TcpListener,
    local_addr: String,
}

impl Listener for NativeListener {
    fn local_addr(&self) -> String {
        self.local_addr.clone()
    }

    fn serve(
        self: Arc<Self>,
        options: ServeOptions,
        handler: Handler,
    ) -> BoxFuture<Result<(), Error>> {
        Box::pin(serve(self, options, handler))
    }
}

/// Run the accept loop until dropped. Dropping it aborts every in-flight
/// connection task (`JoinSet::Drop`).
async fn serve(
    listener: Arc<NativeListener>,
    options: ServeOptions,
    handler: Handler,
) -> Result<(), Error> {
    let acceptor = match &options.tls {
        Some(tls) => Some((
            build_acceptor(tls, options.allow_http1, options.allow_http2)?,
            tls.handshake_timeout,
        )),
        None => None,
    };
    let max_connections = options.max_connections.clamp(1, Semaphore::MAX_PERMITS);
    let (allow_http1, allow_http2, header_read_timeout) = (
        options.allow_http1,
        options.allow_http2,
        options.header_read_timeout,
    );

    let mut conns: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
    let conn_limit = Arc::new(Semaphore::new(max_connections));
    let mut accept_backoff_ms: u64 = 0;
    loop {
        let accepted = tokio::select! {
            result = listener.listener.accept() => result,
            // Reap finished connections so the set doesn't grow unbounded.
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
                tokio::time::sleep(Duration::from_millis(accept_backoff_ms)).await;
                continue;
            }
        };

        // Backpressure: at the cap the accepted socket waits here (and the
        // kernel backlog holds the rest) rather than spawning past the limit.
        let permit = Arc::clone(&conn_limit)
            .acquire_owned()
            .await
            .expect("connection semaphore is never closed");
        while conns.try_join_next().is_some() {}

        let acceptor = acceptor.clone();
        let handler = Arc::clone(&handler);
        conns.spawn(async move {
            // Held for the connection's lifetime. A WebSocket upgrade takes it
            // over so a socket that outlives its HTTP connection keeps
            // counting against `max_connections`.
            let permit: ConnPermit = Arc::new(std::sync::Mutex::new(Some(permit)));
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
                let handler = Arc::clone(&handler);
                let permit = Arc::clone(&permit);
                async move { Ok::<_, Infallible>(handle_request(req, handler, permit).await) }
            });
            serve_connection(io, service, allow_http1, allow_http2, header_read_timeout).await;
        });
    }
}

/// Serve a single connection with the protocol(s) the server allows. Both
/// allowed → negotiate automatically (ALPN for TLS, prefix sniffing for
/// cleartext); otherwise pin to the single allowed protocol.
///
/// The HTTP/1 paths are bound with upgrades enabled, without which
/// `hyper::upgrade::on` never resolves and a WebSocket handshake would answer
/// `101` and then hang. HTTP/2 has no such mode: it carries WebSocket over
/// extended CONNECT (RFC 8441), which this server does not implement.
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
        let _ = builder.serve_connection_with_upgrades(io, service).await;
    } else if allow_http1 {
        let mut builder = hyper::server::conn::http1::Builder::new();
        if let Some(t) = header_read_timeout {
            builder.timer(TokioTimer::new()).header_read_timeout(t);
        }
        let _ = builder.serve_connection(io, service).with_upgrades().await;
    } else if allow_http2 {
        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
            .serve_connection(io, service)
            .await;
    }
}

fn raw_headers(headers: &HeaderMap) -> Headers {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// hyper service body: hand the request to the handler and write its answer.
async fn handle_request(
    mut req: HyperRequest<Incoming>,
    handler: Handler,
    conn_permit: ConnPermit,
) -> HyperResponse<WireBody> {
    let websocket_key = websocket_key(&req);
    let method = req.method().as_str().to_string();
    let url = req.uri().to_string();
    let headers = raw_headers(req.headers());

    if let Some(key) = websocket_key {
        // A handshake carries no body (RFC 6455 §4.1), and reading one would
        // consume the stream the upgrade needs.
        let request = ServerRequest {
            method,
            url,
            headers,
            body: Box::pin(futures::stream::empty()),
            websocket_upgrade: true,
        };
        return match handler(request).await {
            ServerResponse::AcceptWebSocket(on_socket) => {
                // Registered before the response is handed back: hyper resolves
                // this only once the 101 has been written.
                let upgraded = hyper::upgrade::on(&mut req);
                // The socket, not the exchange that created it, occupies the slot.
                let permit = conn_permit
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                tokio::spawn(async move {
                    let _permit = permit;
                    // The client can still disappear between the 101 and the handover.
                    let Ok(upgraded) = upgraded.await else {
                        return;
                    };
                    let stream = WebSocketStream::from_raw_socket(
                        TokioIo::new(upgraded),
                        Role::Server,
                        None,
                    )
                    .await;
                    on_socket(crate::ws::from_tungstenite(stream)).await;
                });
                HyperResponse::builder()
                    .status(101)
                    .header(CONNECTION, "Upgrade")
                    .header(UPGRADE, "websocket")
                    .header(SEC_WEBSOCKET_ACCEPT, derive_accept_key(key.as_bytes()))
                    .body(Empty::new().boxed_unsync())
                    .unwrap_or_else(|_| {
                        status_response(500, "could not build the upgrade response")
                    })
            }
            ServerResponse::Http {
                status,
                headers,
                body,
            } => wire_response(status, &headers, body),
        };
    }

    let body = BodyStream::new(req.into_body()).filter_map(|frame| {
        futures::future::ready(match frame {
            Ok(frame) => frame.into_data().ok().map(Ok),
            Err(e) => Some(Err(Error::io(e.to_string()))),
        })
    });
    let request = ServerRequest {
        method,
        url,
        headers,
        body: Box::pin(body),
        websocket_upgrade: false,
    };
    match handler(request).await {
        ServerResponse::Http {
            status,
            headers,
            body,
        } => wire_response(status, &headers, body),
        ServerResponse::AcceptWebSocket(_) => {
            status_response(500, "cannot accept a WebSocket on a non-upgrade request")
        }
    }
}

/// The `Sec-WebSocket-Key` of an RFC 6455 upgrade handshake, or `None` if `req`
/// is an ordinary request.
///
/// Every condition is required by §4.1, and only all of them together separate
/// an upgrade from a plain `GET` — so a request that merely looks websocket-ish
/// is an ordinary request. HTTP/2 carries WebSocket over extended CONNECT
/// (RFC 8441) instead, which this server does not implement; the `HTTP_11`
/// check is what excludes it.
fn websocket_key(req: &HyperRequest<Incoming>) -> Option<HeaderValue> {
    if req.method() != hyper::Method::GET || req.version() != hyper::Version::HTTP_11 {
        return None;
    }
    if !header_has_token(req.headers(), &CONNECTION, "upgrade")
        || !header_has_token(req.headers(), &UPGRADE, "websocket")
    {
        return None;
    }
    // 13 is the only version this (and every current) implementation speaks.
    if req
        .headers()
        .get(SEC_WEBSOCKET_VERSION)
        .map(HeaderValue::as_bytes)
        != Some(b"13")
    {
        return None;
    }
    req.headers().get(SEC_WEBSOCKET_KEY).cloned()
}

/// Whether any comma-separated token of header `name` equals `token`, ignoring
/// case. Both `Connection` and `Upgrade` are token lists (`keep-alive, Upgrade`),
/// and either may also be split across repeated header fields.
fn header_has_token(headers: &HeaderMap, name: &HeaderName, token: &str) -> bool {
    headers.get_all(name).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(token))
        })
    })
}

/// Write a handler's response. Invalid header names/values are dropped; an
/// out-of-range status becomes a 500.
fn wire_response(status: u16, headers: &Headers, body: ResponseBody) -> HyperResponse<WireBody> {
    let body = match body {
        ResponseBody::Full(bytes) => Full::new(bytes).boxed_unsync(),
        ResponseBody::Stream(chunks) => {
            StreamBody::new(chunks.map(|chunk| Ok::<_, Infallible>(Frame::data(chunk))))
                .boxed_unsync()
        }
    };
    let mut builder = HyperResponse::builder().status(status);
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            builder = builder.header(name, value);
        }
    }
    builder
        .body(body)
        .unwrap_or_else(|_| status_response(500, "invalid response"))
}

/// A small plaintext response for internal error conditions.
fn status_response(status: u16, message: &str) -> HyperResponse<WireBody> {
    HyperResponse::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::copy_from_slice(message.as_bytes())).boxed_unsync())
        .unwrap_or_else(|_| HyperResponse::new(Full::new(Bytes::new()).boxed_unsync()))
}
