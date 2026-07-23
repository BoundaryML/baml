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
function connect(url: string, headers: map<string, string>) -> WsStream throws root.errors.Io {
    $rust_io_function
}
