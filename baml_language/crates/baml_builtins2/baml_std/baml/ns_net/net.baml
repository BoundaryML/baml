/// Converts an optional timeout into the nanosecond count carried across the
/// sys-op boundary. `null` (no deadline) becomes `0n`, which the native layer
/// treats as unbounded (see `timeout_from_nanos`). Modeled on Rust's
/// `Option<Duration>`, where `None` means "block forever".
function _timeout_nanos(timeout: root.time.Duration?) -> bigint throws never {
    match (timeout) {
        null => 0n,
        let t: root.time.Duration => t.to_nanoseconds(),
    }
}

/// A TCP connection. Created by `baml.net.TcpStream.connect` or returned by
/// `baml.net.TcpListener.accept`. Modeled on Rust's `std::net::TcpStream`.
class TcpStream {
    _handle: $rust_type,

    /// Opens a TCP connection to `addr` (e.g. `"127.0.0.1:8080"`) and returns the stream.
    ///
    /// `timeout` bounds how long to wait for the connection to be established;
    /// `null` (the default) waits indefinitely (subject to the OS default). On
    /// expiry this throws `root.errors.Timeout`.
    function connect(
        addr: string,
        timeout: root.time.Duration? = null,
    ) -> TcpStream throws root.errors.Io | root.errors.Timeout {
        // Thin wrapper resolving the default: `$rust_io_function` sys-ops don't fill
        // omitted args, so `connect` forwards to the all-required `_connect` sys-op.
        TcpStream._connect(addr, _timeout_nanos(timeout))
    }

    function _connect(
        addr: string,
        timeout_nanos: bigint,
    ) -> TcpStream throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Reads the next available bytes from the stream. Blocks until data arrives.
    /// Returns an empty `uint8array` once the peer has closed the connection (EOF).
    ///
    /// `timeout` bounds the wait for data; `null` (the default) blocks
    /// indefinitely. On expiry this throws `root.errors.Timeout` and the stream
    /// remains usable for a subsequent read.
    function read(
        self,
        timeout: root.time.Duration? = null,
    ) -> uint8array throws root.errors.Io | root.errors.Timeout {
        self._read(_timeout_nanos(timeout))
    }

    function _read(
        self,
        timeout_nanos: bigint,
    ) -> uint8array throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Writes `data` to the stream. Blocks until every byte has been flushed.
    ///
    /// `timeout` bounds the flush; `null` (the default) blocks indefinitely. On
    /// expiry this throws `root.errors.Timeout`; an unknown amount of `data` may
    /// already have been written.
    function write(
        self,
        data: uint8array,
        timeout: root.time.Duration? = null,
    ) -> null throws root.errors.Io | root.errors.Timeout {
        self._write(data, _timeout_nanos(timeout))
    }

    function _write(
        self,
        data: uint8array,
        timeout_nanos: bigint,
    ) -> null throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Closes the connection.
    function close(self) -> null throws root.errors.Io {
        $rust_io_function
    }
}

/// A bound TCP listener. Created by `baml.net.TcpListener.bind`. Calling `accept`
/// suspends the current BAML thread until an incoming connection arrives (or the
/// thread is cancelled). Modeled on Rust's `std::net::TcpListener`.
class TcpListener {
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
/// Modeled on Rust's `std::net::UdpSocket`.
class UdpSocket {
    _handle: $rust_type,

    /// Binds to `addr` (e.g. `"0.0.0.0:0"` for an OS-assigned ephemeral port) and
    /// returns the socket.
    function bind(addr: string) -> UdpSocket throws root.errors.Io {
        $rust_io_function
    }

    /// Sends `data` as a single datagram to `addr`. Returns the number of bytes sent.
    ///
    /// `timeout` bounds the send; `null` (the default) blocks indefinitely. On
    /// expiry this throws `root.errors.Timeout`. Mirrors `UdpSocket::set_write_timeout`.
    function send_to(
        self,
        data: uint8array,
        addr: string,
        timeout: root.time.Duration? = null,
    ) -> int throws root.errors.Io | root.errors.Timeout {
        self._send_to(data, addr, _timeout_nanos(timeout))
    }

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
    /// indefinitely. On expiry this throws `root.errors.Timeout` and the socket
    /// remains usable. Mirrors `UdpSocket::set_read_timeout`.
    function recv_from(
        self,
        timeout: root.time.Duration? = null,
    ) -> Datagram throws root.errors.Io | root.errors.Timeout {
        self._recv_from(_timeout_nanos(timeout))
    }

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
