// Minimal WebSocket client surface for realtime providers.

/// A WebSocket connection.
class WebSocket {
    _handle: $rust_type,

    /// Send a websocket frame.
    ///
    /// ## Throws
    /// `baml.errors.Io` if:
    /// - The connection is closed
    function send(self, data: string | uint8array) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Blocks until the next frame is received.
    ///
    /// Once the connection is closed (by either party), all subsequent calls
    /// will return `CloseEvent`.
    function next(self) -> string | uint8array | CloseEvent throws root.errors.Io {
        $rust_io_function
    }

    /// Close the connection.
    ///
    /// ## Throws
    /// `baml.errors.InvalidArgument` if `code` is not a valid WebSocket close code
    function close(self, code: int, reason: string) -> void throws root.errors.InvalidArgument {
        $rust_io_function
    }
}

class CloseEvent {
    /// https://developer.mozilla.org/en-US/docs/Web/API/CloseEvent/code
    code: int,
    reason: string,
}

/// Open a WebSocket connection with additional request headers.
function connect(
    url: string,
    headers: map<string, string>,
    timeout: root.time.Duration = root.time.Duration.from_seconds(30),
) -> WebSocket throws root.errors.Io | root.errors.Timeout {
    _connect(url, headers, timeout.to_nanoseconds())
}

function _connect(
    url: string,
    headers: map<string, string>,
    timeout_nanos: bigint,
) -> WebSocket throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}
