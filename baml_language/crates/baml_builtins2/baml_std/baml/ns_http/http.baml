/// An HTTP request — outgoing (built for `baml.http.send`/`fetch_sse`) or
/// incoming (passed to an `http.Server` handler).
///
/// For an incoming server request: `url` is the request-target as received
/// (origin-form, e.g. `/path?q=1`, on HTTP/1; the authority lives in the `host`
/// header), `body` is the bytes decoded lossily as UTF-8 (non-UTF-8 bytes become
/// U+FFFD), and a header sent multiple times has its values joined with `, `.
class Request {
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

/// A Server-Sent Events (SSE) stream. Created by `baml.http.fetch_sse`.
class SseStream {
    url: string,
    /// Status code of the opening response. `fetch_sse` throws on non-2xx,
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

/// Converts an optional timeout into the nanosecond count carried across the
/// sys-op boundary. `null` (no deadline) becomes `0n`, which the native layer
/// treats as unbounded. Modeled on Rust's `Option<Duration>`.
function _timeout_nanos(timeout: root.time.Duration?) -> bigint throws never {
    match (timeout) {
        null => 0n,
        let t: root.time.Duration => t.to_nanoseconds(),
    }
}

/// Sends a GET request to `url` and returns the response.
///
/// `timeout` is the total deadline for the request (connection + response
/// headers + body). `null` (the default) imposes no deadline. On expiry this
/// throws `root.errors.Timeout`.
function fetch(
    url: string,
    timeout: root.time.Duration? = null,
) -> Response throws root.errors.Io | root.errors.Timeout {
    // Thin wrapper resolving the default: `$rust_io_function` sys-ops don't fill
    // omitted args, so `fetch` forwards to the all-required `_fetch` sys-op.
    _fetch(url, _timeout_nanos(timeout))
}

function _fetch(
    url: string,
    timeout_nanos: bigint,
) -> Response throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}

/// Sends an HTTP request and returns the response.
///
/// `timeout` is the total deadline for the request, as in `fetch`. `null` (the
/// default) imposes no deadline.
///
/// # Throws
///
/// - `InvalidArgument` if `request.method` is not a valid HTTP method.
/// - `Io` if the request cannot be sent or the response cannot be read.
/// - `Timeout` if `timeout` elapses first.
function send(
    request: Request,
    timeout: root.time.Duration? = null,
) -> Response throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    _send(request, _timeout_nanos(timeout))
}

function _send(
    request: Request,
    timeout_nanos: bigint,
) -> Response throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}

/// Sends an HTTP request and returns a Server-Sent Events stream for streaming responses.
///
/// On native transports, `timeout` covers the request and streaming body.
/// `first_event_timeout` starts after open and ends on the first parsed event.
/// Null/non-positive is unbounded; browser/Wasm does not enforce these fields.
///
/// # Throws
///
/// - `InvalidArgument` if `request.method` is not a valid HTTP method.
/// - `Io` if the request cannot be sent or the stream cannot be opened.
/// - `Timeout` if `timeout` or `first_event_timeout` elapses first.
function fetch_sse(
    request: Request,
    timeout: root.time.Duration? = null,
    first_event_timeout: root.time.Duration? = null,
) -> SseStream throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    _fetch_sse(request, _timeout_nanos(timeout), _timeout_nanos(first_event_timeout))
}

function _fetch_sse(
    request: Request,
    timeout_nanos: bigint,
    first_event_timeout_nanos: bigint,
) -> SseStream throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}
