class Socket {
  _handle $rust_type

  function read(self) -> string throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
  }

  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }
}

function connect(addr: string) -> Socket throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}
