// A minimal WebSocket client surface, modeled on `ns_http`'s `SseStream`: an opaque
// `$rust_type` handle behind an async-aware split sink/stream, `next()` awaiting the
// next text frame, and `close()` releasing the connection. Text frames only for now
// (Binary frames are lossily UTF-8 decoded; Ping/Pong are handled transparently by
// tungstenite). The duplex transport this unlocks makes realtime providers
// run live (scenario 22).

/// A WebSocket connection. Text frames only for now.
class WsStream {
    _handle: $rust_type,

    /// Send a text frame.
    function send(self, text: string) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Receive the next text frame; null when the connection closes. Blocks until a
    /// frame arrives. Binary frames are decoded lossily as UTF-8; Ping/Pong control
    /// frames are answered by the transport and skipped.
    function next(self) -> string? throws root.errors.Io {
        $rust_io_function
    }

    /// Close the connection.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// Open a WebSocket connection (`ws://` or `wss://`) with extra request headers.
function connect(url: string, headers: map<string, string>) -> WsStream throws root.errors.Io {
    $rust_io_function
}
