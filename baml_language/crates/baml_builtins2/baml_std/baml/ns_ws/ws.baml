// Minimal text-oriented WebSocket client surface for realtime providers.

/// A WebSocket connection.
class WsStream {
    _handle: $rust_type,

    /// Send a text frame.
    function send(self, text: string) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Receive the next text frame, or `null` after the connection closes.
    function next(self) -> string? throws root.errors.Io {
        $rust_io_function
    }

    /// Close the connection.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// Open a WebSocket connection with additional request headers.
function connect(
    url: string,
    headers: map<string, string>,
    timeout: root.time.Duration = root.time.Duration.from_seconds(30),
) -> WsStream throws root.errors.Io | root.errors.Timeout {
    _connect(url, headers, timeout.to_nanoseconds())
}

function _connect(
    url: string,
    headers: map<string, string>,
    timeout_nanos: bigint,
) -> WsStream throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}
