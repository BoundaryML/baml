/// A TCP connection. Created by `baml.net.TcpStream.connect` or returned by
/// `baml.net.TcpListener.accept`. Modeled on Rust's `std::net::TcpStream`.
class TcpStream {
  _handle $rust_type

  /// Opens a TCP connection to `addr` (e.g. `"127.0.0.1:8080"`) and returns the stream.
  function connect(addr: string) -> TcpStream throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
  }

  /// Reads the next available bytes from the stream. Blocks until data arrives.
  /// Returns an empty `uint8array` once the peer has closed the connection (EOF).
  function read(self) -> uint8array throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
  }

  /// Writes `data` to the stream. Blocks until every byte has been flushed.
  function write(self, data: uint8array) -> null throws root.errors.Io | root.errors.Timeout {
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
  _handle $rust_type

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
  _handle $rust_type

  /// Binds to `addr` (e.g. `"0.0.0.0:0"` for an OS-assigned ephemeral port) and
  /// returns the socket.
  function bind(addr: string) -> UdpSocket throws root.errors.Io {
    $rust_io_function
  }

  /// Sends `data` as a single datagram to `addr`. Returns the number of bytes sent.
  function send_to(self, data: uint8array, addr: string) -> int throws root.errors.Io {
    $rust_io_function
  }

  /// Receives a single datagram, returning its payload bytes together with the
  /// address of the sender.
  function recv_from(self) -> Datagram throws root.errors.Io {
    $rust_io_function
  }

  /// Closes the socket.
  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }
}

/// A received UDP datagram: the payload bytes and the address of the sender.
class Datagram {
  data uint8array
  addr string
}
