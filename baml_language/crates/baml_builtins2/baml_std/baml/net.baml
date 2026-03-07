class Socket {
  _handle $rust_type

  function read(self) -> string throws baml.errors.Io | baml.errors.Timeout {
    $rust_io_function
  }

  function close(self) -> null throws baml.errors.Io {
    $rust_io_function
  }
}

function connect(addr: string) -> Socket throws baml.errors.Io | baml.errors.Timeout {
  $rust_io_function
}
