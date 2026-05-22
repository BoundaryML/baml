/// An HTTP request to be sent with `baml.http.send` or `baml.http.fetch_sse`.
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
  function close(self) -> null {
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
