/// A minimal HTTP/HTTPS server.
///
/// Bind a listening socket with `Server.bind`, then call `serve` with a request
/// handler to start serving:
/// ```baml
/// let server = baml.http.Server.bind("127.0.0.1:0");
/// spawn { server.serve((req) -> { baml.http.Response.new(200, { }, "hi".to_utf8()) }) };
/// // server.addr now holds the actual bound address, e.g. "127.0.0.1:54321".
/// ```
class Server {
    /// The resolved local address the listener is bound to, e.g.
    /// `"127.0.0.1:54321"`. Use it to discover the port after binding `":0"`.
    addr: string,
    _state: $rust_type,

    /// Binds a TCP listener on `addr` and returns a `Server` ready to `serve`.
    /// `":0"` (e.g. `"127.0.0.1:0"`) asks the OS for an ephemeral port; the chosen
    /// address is available as `.addr`. The socket starts queuing incoming
    /// connections immediately, so a client may connect before `serve` is called.
    ///
    /// # Parameters
    /// - `addr`: The address to bind. E.g. `"127.0.0.1:8080"` or `"127.0.0.1:0"`.
    function bind(addr: string) -> Server throws root.errors.Io {
        $rust_io_function
    }

    /// Serves requests with `handler`, blocking the current thread until it is
    /// cancelled. To run it in the background, use `spawn { server.serve(...) }`.
    ///
    /// Each incoming request is dispatched on its own BAML thread, so requests are
    /// handled concurrently (including multiplexed HTTP/2 streams on one
    /// connection). Each request is isolated: if the `handler` panics or its
    /// response can't be written (e.g. a client that hung up early), only that
    /// request fails — the client receives a `500` and the server keeps serving.
    ///
    /// A `Server` serves one handler at a time: calling `serve` while it is already
    /// serving raises `Io`. Once a serve ends (its thread is cancelled), the same
    /// `Server` may be served again — with a different `handler`, `tls_config`, or
    /// protocol set if desired. The bound port is held until the `Server` itself is
    /// dropped, not when a serve ends.
    ///
    /// # Parameters
    /// - `handler`: Turns each incoming `Request` into a `Response`.
    /// - `websocket`: Handles WebSocket upgrade requests (see `WsAccept`). Returning a
    ///   `Response` refuses the upgrade and serves that response; returning a `WsAccept`
    ///   completes the handshake and runs it with the connected socket. The default
    ///   refuses every upgrade with `501`.
    /// - `tls_config`: Optional TLS configuration. If provided, the server behaves as an HTTPS server.
    /// - `allow_http1` / `allow_http2`: Which HTTP versions to accept (at least one must be true).
    /// - `max_body_size`: Maximum request body size, in bytes, buffered per request
    ///   (default 100 MiB). A larger body is rejected with `413 Payload Too Large`,
    ///   bounding per-request memory.
    /// - `max_connections`: Maximum number of connections served concurrently
    ///   (default 1024). At the cap the accept loop applies backpressure (new
    ///   connections wait in the kernel backlog) rather than spawning unbounded tasks.
    ///   An accepted WebSocket holds its slot for as long as the socket lives, not
    ///   just for the handshake.
    /// - `header_read_timeout`: How long a client has to send the complete request
    ///   headers before the connection is closed (default 30s) — a Slowloris defense.
    ///   A non-positive duration disables it. (HTTP/1 only; HTTP/2 frames its headers.)
    ///
    /// # WebSockets
    /// A request is dispatched to `websocket` instead of `handler` only when it is a
    /// complete RFC 6455 handshake — an HTTP/1.1 `GET` carrying `Connection: Upgrade`,
    /// `Upgrade: websocket`, and `Sec-WebSocket-Version: 13`. Anything short of that,
    /// including WebSocket over HTTP/2 (RFC 8441 extended CONNECT, unsupported here),
    /// is an ordinary request and goes to `handler`.
    ///
    /// The handshake response is built by the server, so a `WsAccept` cannot add
    /// headers to it: `Sec-WebSocket-Protocol` subprotocols and extensions are not
    /// negotiated.
    ///
    /// # Panics
    /// - Calling `serve` on an unsupported platform (e.g. in the browser) will panic.
    /// - If the serving thread is cancelled, `serve` ends with a
    ///   `baml.panics.Cancelled` panic and any in-flight connections are dropped.
    function serve(
        self,
        handler: (req: Request) -> Response throws never,
        websocket: (req: Request) -> Response | WsAccept throws never = reject_websocket,
        tls_config: TlsConfig? = null,
        allow_http1: bool = true,
        allow_http2: bool = true,
        max_body_size: int = 104857600,
        max_connections: int = 1024,
        header_read_timeout: root.time.Duration = root.time.Duration.from_seconds(30n),
    ) -> never throws root.errors.Io {
        // `$rust_io_function` sys-ops don't fill omitted default args, so this
        // thin BAML wrapper resolves the defaults and forwards to the `_serve`
        // sys-op (whose params are all required).
        self._serve(
            handler,
            websocket,
            tls_config,
            allow_http1,
            allow_http2,
            max_body_size,
            max_connections,
            header_read_timeout.to_nanoseconds(),
        )
    }

    function _serve(
        self,
        handler: (req: Request) -> Response throws never,
        websocket: (req: Request) -> Response | WsAccept throws never,
        tls_config: TlsConfig?,
        allow_http1: bool,
        allow_http2: bool,
        max_body_size: int,
        max_connections: int,
        header_read_timeout_nanos: bigint,
    ) -> never throws root.errors.Io {
        $rust_io_function
    }
}

/// Takes over an accepted WebSocket connection.
///
/// A `serve` `websocket` handler returns one of these to complete the upgrade.
/// It runs on its own BAML thread once the handshake finishes, and owns the
/// connection: when it returns, the socket is closed.
///
/// ```baml
/// // Echo every frame back until the peer closes.
/// spawn {
///     server.serve(my_handler, websocket = (req) -> {
///        // always accept websocket requests:
///        (socket) -> {
///            while (true) {
///                match (socket.next()) {
///                    baml.ws.CloseEvent => { break; },
///                    let frame: string | uint8array => socket.send(frame),
///                }
///            }
///        }
///     })
/// };
/// ```
type WsAccept = (socket: root.ws.WebSocket) -> void throws never;

/// The `websocket` handler `serve` uses when none is supplied: refuse every
/// upgrade, leaving the server HTTP-only.
///
/// RFC 6455 §4.1 fails the client handshake on any status other than `101`; the
/// status here says specifically that this server implements no WebSocket
/// endpoint, rather than blaming the request.
function reject_websocket(req: Request) -> Response throws never {
    Response.new(501, { }, "websocket upgrade is not supported".to_utf8())
}

/// Used to convert an HTTP server into an HTTPS server for secure connections.
///
/// To force HTTP clients onto HTTPS, run two servers — a plaintext one on the
/// HTTP port whose handler redirects, and the TLS one on the HTTPS port:
/// ```baml
/// let secure = http.Server.bind("0.0.0.0:443");
/// let upgrade = http.Server.bind("0.0.0.0:80");
/// spawn { upgrade.serve((req) -> {
///     baml.http.Response.new(308, { "Location": "https://" + req.headers.get("host") + req.url }, "".to_utf8())
/// }) };
/// spawn { secure.serve(my_handler, tls_config = my_tls) };
/// ```
class TlsConfig {
    /// Whether to allow connections using TLSv1.2.
    /// This is enabled by default, but may be disabled to require TLSv1.3.
    ///
    /// BAML's standard HTTPS server does not provide TLS/SSL versions lower than TLSv1.2 as they are insecure.
    allow_tls1_2: bool,
    _certificate: $rust_type,
    _private_key: $rust_type,
    _handshake_timeout_nanos: bigint,

    /// Parses a PEM-encoded certificate chain and private key into a TLS configuration.
    ///
    /// # Parameters
    /// - `cert_pem`: PEM-encoded certificate chain (leaf certificate first).
    /// - `key_pem`: PEM-encoded private key (PKCS#8, PKCS#1/RSA, or SEC1/EC).
    /// - `handshake_timeout`: How long a TLS handshake may take before the
    ///   connection is dropped (default 10s), bounding a client that opens a
    ///   socket then stalls mid-handshake. A non-positive duration disables it.
    function new(
        cert_pem: uint8array,
        key_pem: uint8array,
        allow_tls1_2: bool = true,
        handshake_timeout: root.time.Duration = root.time.Duration.from_seconds(10n),
    ) -> TlsConfig throws root.errors.Io {
        // `$rust_io_function` sys-ops don't fill omitted default args, so this
        // wrapper resolves the defaults and forwards to `_new`.
        TlsConfig._new(cert_pem, key_pem, allow_tls1_2, handshake_timeout.to_nanoseconds())
    }

    function _new(
        cert_pem: uint8array,
        key_pem: uint8array,
        allow_tls1_2: bool,
        handshake_timeout_nanos: bigint,
    ) -> TlsConfig throws root.errors.Io {
        $rust_io_function
    }
}
