/// An outgoing HTTP request, sent with `baml.http.send` or `baml.http.send_sse`.
///
/// Build one as a literal, or with `Request.from_url` and the `set_*` methods:
/// ```baml
/// let req = baml.http.Request.from_url("https://api.example.com/items")
///     .set_method("POST")
///     .set_headers({ "content-type": "application/json" })
///     .set_body("{}")
///     .set_timeout(baml.time.Duration.from_seconds(10));
/// let resp = baml.http.send(req);
/// ```
///
/// An `http.Server` handler receives a `ServerRequest`, not this class.
class Request {
    method: string,
    url: string,
    headers: map<string, string>,
    body: string,
    /// End-to-end limit for the whole exchange: connecting, sending the
    /// request, waiting for response headers and reading the body. `null` is
    /// the default of 5 minutes; a zero or negative duration is no limit.
    timeout: root.time.Duration?,
    /// Limit for establishing the connection (TCP and TLS). `null` is the
    /// default of 10 seconds; a zero or negative duration is no limit.
    connect_timeout: root.time.Duration?,

    /// A request for `url` with no method, headers or body. Set the method with
    /// `set_method` before sending: a request with no method is rejected with
    /// `InvalidArgument`.
    function from_url(url: string) -> Request throws never {
        Request {
            method: "",
            url: url,
            headers: {},
            body: "",
            timeout: null,
            connect_timeout: null,
        }
    }

    /// Sets the HTTP method, e.g. `"GET"` or `"POST"`. Mutates and returns `self`.
    function set_method(self, method: string) -> Request throws never {
        self.method = method;
        self
    }

    /// Replaces all headers with `headers`. Mutates and returns `self`.
    function set_headers(self, headers: map<string, string>) -> Request throws never {
        self.headers = headers;
        self
    }

    /// Sets the request body. Mutates and returns `self`.
    function set_body(self, body: string) -> Request throws never {
        self.body = body;
        self
    }

    /// Sets the end-to-end limit (see `timeout`). Mutates and returns `self`.
    function set_timeout(self, timeout: root.time.Duration?) -> Request throws never {
        self.timeout = timeout;
        self
    }

    /// Sets the connection limit (see `connect_timeout`). Mutates and returns `self`.
    function set_connect_timeout(
        self,
        connect_timeout: root.time.Duration?,
    ) -> Request throws never {
        self.connect_timeout = connect_timeout;
        self
    }
}

/// An incoming HTTP request, passed to an `http.Server` handler.
///
/// `url` is the request-target as received (origin-form, e.g. `/path?q=1`, on
/// HTTP/1; the authority lives in the `host` header), `body` is the bytes
/// decoded lossily as UTF-8 (non-UTF-8 bytes become U+FFFD), and a header sent
/// multiple times has its values joined with `, `.
class ServerRequest {
    method: string,
    url: string,
    headers: map<string, string>,
    body: string,
}

/// An HTTP response returned by `baml.http.fetch` or `baml.http.send`.
class Response {
    /// The [HTTP response status code](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status)
    status_code: int,
    /// The [HTTP response headers](https://developer.mozilla.org/en-US/docs/Glossary/Response_header)
    headers: map<string, string>,
    /// The URL the response came from.
    url: string,
    _body: $rust_type,

    /// Returns the response body as a string. Live client responses are decoded
    /// lossily as UTF-8, replacing malformed bytes with U+FFFD. Buffered `Bytes`
    /// responses use strict UTF-8 decoding.
    /// Throws `Timeout` if the request deadline elapses while reading the body,
    /// or `Io` if the body was unavailable or buffered bytes were not valid UTF-8.
    function text(self) -> string throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Returns the response body as raw bytes.
    /// Throws `Timeout` if the request deadline elapses while reading the body,
    /// or `Io` if the body was unavailable.
    function bytes(self) -> uint8array throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Returns `true` if `status_code` is in the 200–299 range.
    function ok(self) -> bool {
        self.status_code >= 200 && self.status_code < 300
    }

    /// Builds a response, typically to return from a `baml.http.Server` handler.
    ///
    /// `body` is sent verbatim as the response body. `headers` should include a
    /// `Content-Type` if the client needs one; `Content-Length` is set for you.
    function new(
        status_code: int,
        headers: map<string, string>,
        body: uint8array,
    ) -> Response throws never {
        $rust_io_function
    }

    /// Builds a *streaming* response to return from an `http.Server` handler: the
    /// status line and `headers` are sent immediately, then the body is written
    /// incrementally with successive `write` calls and ended with `end`. The
    /// response is framed with chunked transfer-encoding, so each `write` is
    /// flushed to the client as its own chunk — a slow producer streams in real
    /// time (e.g. for Server-Sent Events). Do not set `Content-Length`; it is
    /// omitted for a streamed body.
    ///
    /// Because hyper only reads the response body *after* the handler returns, the
    /// writes must run after the handler hands the response back — typically from a
    /// `spawn`ed task that captures it, mirroring hyper's channel-backed body:
    /// ```baml
    /// let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
    /// spawn {
    ///   resp.write("data: one\n\n".to_utf8());
    ///   resp.write("data: two\n\n".to_utf8());
    ///   resp.end();
    /// };
    /// resp
    /// ```
    function new_streaming(
        status_code: int,
        headers: map<string, string>,
    ) -> Response throws never {
        $rust_io_function
    }

    /// Writes one chunk to a streaming response body (see `new_streaming`),
    /// flushing it to the client. Applies backpressure: `write` suspends until the
    /// previous chunk has been accepted by the connection. Raises `Io` if the
    /// response was not created with `new_streaming`, if it has been `end`ed, or
    /// if the client has hung up.
    function write(self, data: uint8array) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Ends a streaming response body (see `new_streaming`), signaling
    /// end-of-stream to the client so the chunked response completes. Subsequent
    /// `write` calls raise `Io`. A no-op on an already-ended response.
    function end(self) -> null throws root.errors.Io {
        $rust_io_function
    }
}

/// A Server-Sent Events (SSE) stream. Created by `baml.http.send_sse`.
class SseStream {
    url: string,
    /// Status code of the opening response. `send_sse` throws on non-2xx,
    /// so this is always a success status.
    status_code: int,
    /// Response headers at connection open — the provider request id
    /// (`x-request-id`, `request-id`, ...) lives here.
    headers: map<string, string>,
    _handle: $rust_type,

    /// Only returns `null` if the stream is done/closed.
    /// Otherwise, waits until a new event is available.
    function next(self) -> string? throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Closes the SSE stream and releases the underlying connection.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// (internal) The limit a request runs under, as the nanosecond count carried
/// across the sys-op boundary: `timeout`, else `default` when it is null. The
/// native layer treats zero or negative as no limit.
function _timeout_nanos(
    timeout: root.time.Duration?,
    default: root.time.Duration,
) -> bigint throws never {
    (timeout ?? default).to_nanoseconds()
}

/// (internal) The end-to-end limit used when neither the `timeout` argument nor
/// `Request.timeout` is set.
function _default_timeout() -> root.time.Duration throws never {
    root.time.Duration.from_seconds(300)
}

/// (internal) The connection limit used when `Request.connect_timeout` is not set.
function _default_connect_timeout() -> root.time.Duration throws never {
    root.time.Duration.from_seconds(10)
}

/// Sends a GET request to `url` and returns the response.
///
/// `timeout` is the end-to-end limit for the request: connecting, waiting for
/// response headers and reading the body. `null` (the default) is 5 minutes; a
/// zero or negative duration is no limit. Connecting is also limited to 10
/// seconds; use `send` with a `Request` to change that. On expiry this throws
/// `baml.errors.Timeout`, whose `timeout_type` is `"timeout"` or
/// `"connect_timeout"`.
function fetch(
    url: string,
    timeout: root.time.Duration? = null,
) -> Response throws root.errors.Io | root.errors.Timeout {
    // Thin wrapper resolving the default: `$rust_io_function` sys-ops don't fill
    // omitted args, so `fetch` forwards to the all-required `_fetch` sys-op.
    _fetch(url, _timeout_nanos(timeout, _default_timeout()), _default_connect_timeout().to_nanoseconds())
}

function _fetch(
    url: string,
    timeout_nanos: bigint,
    connect_timeout_nanos: bigint,
) -> Response throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}

/// Sends an HTTP request and returns the response.
///
/// The request runs under `request.timeout` (end to end, 5 minutes when null)
/// and `request.connect_timeout` (10 seconds when null). The `timeout`
/// argument, when given, overrides `request.timeout`. A zero or negative
/// duration is no limit.
///
/// # Throws
///
/// - `InvalidArgument` if `request.method` is not a valid HTTP method.
/// - `Io` if the request cannot be sent or the response cannot be read.
/// - `Timeout` if a limit elapses first. Its `timeout_type` is `"timeout"` or
///   `"connect_timeout"`, the name of the field that set the limit.
function send(
    request: Request,
    timeout: root.time.Duration? = null,
) -> Response throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    _send(
        request,
        _timeout_nanos(timeout ?? request.timeout, _default_timeout()),
        _timeout_nanos(request.connect_timeout, _default_connect_timeout()),
    )
}

function _send(
    request: Request,
    timeout_nanos: bigint,
    connect_timeout_nanos: bigint,
) -> Response throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}

/// Sends an HTTP request and returns a Server-Sent Events stream for streaming responses.
///
/// The limits are the ones `send` applies: `request.timeout` (overridden by the
/// `timeout` argument) covers opening the stream and reading all of it, and
/// `request.connect_timeout` covers connecting. `SseStream.next()` throws
/// `Timeout` once the end-to-end limit elapses. Browser/Wasm does not enforce
/// these limits.
///
/// # Throws
///
/// - `InvalidArgument` if `request.method` is not a valid HTTP method.
/// - `Io` if the request cannot be sent or the stream cannot be opened.
/// - `Timeout` if a limit elapses first.
function send_sse(
    request: Request,
    timeout: root.time.Duration? = null,
) -> SseStream throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    _send_sse(
        request,
        _timeout_nanos(timeout ?? request.timeout, _default_timeout()),
        _timeout_nanos(request.connect_timeout, _default_connect_timeout()),
    )
}

function _send_sse(
    request: Request,
    timeout_nanos: bigint,
    connect_timeout_nanos: bigint,
) -> SseStream throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}
