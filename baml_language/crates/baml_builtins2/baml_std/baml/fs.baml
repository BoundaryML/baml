class File {
  _handle $rust_type

  function read(self) -> string throws baml.errors.Io {
    $rust_io_function
  }

  function close(self) -> null throws baml.errors.Io {
    $rust_io_function
  }
}

function open(path: string) -> File throws baml.errors.Io {
  $rust_io_function
}
