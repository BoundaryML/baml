// Minimal WebSocket client surface for realtime providers.

/// A WebSocket connection.
class WebSocket {
    _handle: $rust_type,

    /// Send one frame: a `string` becomes a text frame, a `uint8array` a
    /// binary one.
    ///
    /// # Throws
    ///
    /// - `baml.errors.Io` if the connection is already closed, or the send
    ///   fails.
    function send(self, data: string | uint8array) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Block until the next frame arrives.
    ///
    /// A text frame is a `string` and a binary frame a `uint8array`. Pings are
    /// answered here automatically, and neither ping nor pong frames surface.
    /// Once the connection ends — closed by either party, or dropped — this
    /// returns the same `CloseEvent` on every call, so a receive loop
    /// terminates on the first one rather than blocking forever.
    ///
    /// # Throws
    ///
    /// - `baml.errors.Io` if the connection fails while receiving.
    function next(self) -> string | uint8array | CloseEvent throws root.errors.Io {
        $rust_io_function
    }

    /// Begin the closing handshake, sending `code` and `reason` to the peer.
    ///
    /// This returns as soon as the close frame is queued. The connection ends
    /// when the peer echoes it, which `next` reports as a `CloseEvent`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `code` is not one an endpoint may
    ///   send (RFC 6455: 1000-1003, 1007-1013, 3000-3999, 4000-4999; not the
    ///   undefined codes or the inferred-only 1004/1005/1006/1015), or if
    ///   `reason` exceeds 123 bytes, the most a close frame can carry.
    function close(self, code: int, reason: string) -> void throws root.errors.InvalidArgument {
        $rust_io_function
    }
}

/// How a connection ended, as reported by `WebSocket.next`. Mirrors the
/// browser's [`CloseEvent`](https://developer.mozilla.org/en-US/docs/Web/API/CloseEvent).
class CloseEvent {
    /// The RFC 6455 status code. Two are synthesized rather than received:
    /// `1005` when the peer's close frame carried no code, and `1006` when the
    /// connection dropped with no closing handshake at all.
    code: int,
    /// The peer's reason, or `""` if it gave none.
    reason: string,
}

/// Open a WebSocket connection with additional request headers.
///
/// # Throws
///
/// - `baml.errors.Io` if the connection or the opening handshake fails.
/// - `baml.errors.Timeout` if neither completes within `timeout`.
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
