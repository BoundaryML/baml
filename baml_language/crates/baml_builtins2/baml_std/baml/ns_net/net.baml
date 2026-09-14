/// A TCP connection. Created by `baml.net.TcpStream.connect` or returned by
/// `baml.net.TcpListener.accept`.
///
/// Implements `baml.io.Read` and `baml.io.Write`.
class TcpStream {
    /// (internal)
    _handle: $rust_type,

    /// Opens a TCP connection to `addr` (e.g. `"127.0.0.1:8080"`) and returns the stream.
    ///
    /// `timeout` bounds how long to wait for the connection to be established;
    /// `null` (the default) waits indefinitely (subject to the OS default). On
    /// expiry this throws `baml.errors.Timeout`.
    function connect(
        addr: string,
        timeout: root.time.Duration? = null,
    ) -> TcpStream throws root.errors.Io | root.errors.Timeout {
        // Thin wrapper resolving the default: `$rust_io_function` sys-ops don't fill
        // omitted args, so `connect` forwards to the all-required `_connect` sys-op.
        TcpStream._connect(addr, timeout?.to_nanoseconds() ?? 0n)
    }

    /// (internal)
    function _connect(
        addr: string,
        timeout_nanos: bigint,
    ) -> TcpStream throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    implements root.io.Read {
        /// Read at most `limit` bytes. Blocks until data or EOF is available.
        function read(self, limit: int) -> uint8array? throws root.errors.Io {
            $rust_io_function
        }
    }

    implements root.io.Write {
        /// Attempt one write and return the number of bytes accepted.
        function write_some(self, data: uint8array) -> int throws root.errors.Io {
            $rust_io_function
        }

        /// No-op because TCP streams are unbuffered.
        function flush(self) -> void throws never {}
    }

    /// Closes the connection.
    function close(self) -> null throws root.errors.Io {
        $rust_io_function
    }
}

/// A bound TCP listener. Created by `baml.net.TcpListener.bind`. Calling `accept`
/// suspends the current BAML thread until an incoming connection arrives (or the
/// thread is cancelled).
///
/// # Examples
///
/// ```baml
/// let listener = baml.net.TcpListener.bind("127.0.0.1:8080");
/// while (true) {
///     let conn: baml.net.TcpStream = listener.accept();
///     spawn {
///         connection_handler(conn);
///     }
/// }
/// ```
class TcpListener {
    /// (internal)
    _handle: $rust_type,

    /// Binds to `addr` (e.g. `"127.0.0.1:8080"`) and returns a listener ready to
    /// accept incoming connections.
    function bind(addr: string) -> TcpListener throws root.errors.Io {
        $rust_io_function
    }

    /// Waits for the next incoming connection and returns it as a `TcpStream`.
    function accept(self) -> TcpStream throws root.errors.Io {
        $rust_io_function
    }

    function close(self) -> null throws root.errors.Io {
        $rust_io_function
    }
}

/// A UDP socket. Created by `baml.net.UdpSocket.bind`. UDP is connectionless and
/// message-oriented: every `send_to` / `recv_from` carries a single datagram.
class UdpSocket {
    /// (internal)
    _handle: $rust_type,

    /// Binds to `addr` (e.g. `"0.0.0.0:0"` for an OS-assigned ephemeral port) and
    /// returns the socket.
    function bind(addr: string) -> UdpSocket throws root.errors.Io {
        $rust_io_function
    }

    /// Sends `data` as a single datagram to `addr`. Returns the number of bytes sent.
    ///
    /// `timeout` bounds the send; `null` (the default) blocks indefinitely. On
    /// expiry this throws `baml.errors.Timeout`. Mirrors `UdpSocket::set_write_timeout`.
    function send_to(
        self,
        data: uint8array,
        addr: string,
        timeout: root.time.Duration? = null,
    ) -> int throws root.errors.Io | root.errors.Timeout {
        self._send_to(data, addr, timeout?.to_nanoseconds() ?? 0n)
    }

    /// (internal)
    function _send_to(
        self,
        data: uint8array,
        addr: string,
        timeout_nanos: bigint,
    ) -> int throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Receives a single datagram, returning its payload bytes together with the
    /// address of the sender.
    ///
    /// `timeout` bounds the wait for a datagram; `null` (the default) blocks
    /// indefinitely. On expiry this throws `baml.errors.Timeout` and the socket
    /// remains usable. Mirrors `UdpSocket::set_read_timeout`.
    function recv_from(
        self,
        timeout: root.time.Duration? = null,
    ) -> Datagram throws root.errors.Io | root.errors.Timeout {
        self._recv_from(timeout?.to_nanoseconds() ?? 0n)
    }

    /// (internal)
    function _recv_from(
        self,
        timeout_nanos: bigint,
    ) -> Datagram throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Closes the socket.
    function close(self) -> null throws root.errors.Io {
        $rust_io_function
    }
}

/// A received UDP datagram: the payload bytes and the address of the sender.
class Datagram {
    data: uint8array,
    addr: string,
}
