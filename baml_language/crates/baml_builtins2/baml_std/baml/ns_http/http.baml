/// An HTTP request — outgoing (built for `baml.http.send`/`fetch_sse`) or
/// incoming (passed to an `http.Server` handler).
///
/// For an incoming server request: `url` is the request-target as received
/// (origin-form, e.g. `/path?q=1`, on HTTP/1; the authority lives in the `host`
/// header), `body` is the bytes decoded lossily as UTF-8 (non-UTF-8 bytes become
/// U+FFFD), and a header sent multiple times has its values joined with `, `.
class Request {
  method string
  url string
  headers map<string, string>
  body string
}

/// An HTTP response returned by `baml.http.fetch` or `baml.http.send`.
class Response {
  status_code int
  headers map<string, string>
  url string
  _body $rust_type

  /// Returns the response body as a UTF-8 string.
  function text(self) -> string throws root.errors.Io {
    $rust_io_function
  }

  /// Returns the response body as raw bytes.
  function bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  /// Returns `true` if `status_code` is in the 200–299 range.
  function ok(self) -> bool {
    self.status_code >= 200 && self.status_code < 300
  }

  /// Builds a response to return from an `http.Server` handler.
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
}

/// A Server-Sent Events (SSE) stream. Created by `baml.http.fetch_sse`.
class SseStream {
  url string
  _handle $rust_type

  /// Only returns `null` if the stream is done/closed.
  /// Otherwise, waits until a new event is available.
  function next(self) -> string? throws root.errors.Io {
    $rust_io_function
  }

  /// Closes the SSE stream and releases the underlying connection.
  function close(self) -> null throws never {
    $rust_io_function
  }
}

/// Sends a GET request to `url` and returns the response.
function fetch(url: string) -> Response throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

/// Sends an HTTP request and returns the response.
function send(request: Request) -> Response throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

/// Sends an HTTP request and returns a Server-Sent Events stream for streaming responses.
function fetch_sse(request: Request) -> SseStream throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}
