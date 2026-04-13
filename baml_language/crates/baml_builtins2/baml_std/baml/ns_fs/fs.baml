class File {
  _handle $rust_type

  function read_string(self) -> string throws root.errors.Io {
    $rust_io_function
  }

  function read_bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }
}

function open(path: string) -> File throws root.errors.Io {
  $rust_io_function
}
