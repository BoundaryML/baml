/// A TCP socket connection. Created by `baml.net.connect`.
class Socket {
  _handle $rust_type

  /// Reads available data from the socket and returns it as a UTF-8 string. Blocks until data arrives.
  function read(self) -> string throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
  }

  /// Write `data` to the socket. Blocks until every byte has been flushed.
  function write(self, data: string) -> null throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
  }

  /// Closes the socket connection.
  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }
}

/// A bound TCP listener. Created by [`listen`]. Calling `accept` suspends the
/// current BAML thread until an incoming connection arrives (or the thread is
/// cancelled).
class TcpListener {
  _handle $rust_type

  /// Wait for the next incoming connection and return it as a `Socket`.
  function accept(self) -> Socket throws root.errors.Io {
    $rust_io_function
  }

  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }
}

/// Opens a TCP connection to `addr` (e.g. `"127.0.0.1:8080"`) and returns a `Socket`.
function connect(addr: string) -> Socket throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

/// Bind to `addr` (e.g. `"127.0.0.1:8080"`) and return a `TcpListener` ready
/// to accept incoming connections.
function listen(addr: string) -> TcpListener throws root.errors.Io {
  $rust_io_function
}
