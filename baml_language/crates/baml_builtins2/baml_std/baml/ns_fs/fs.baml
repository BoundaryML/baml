class File {
  _handle $rust_type

  function text(self) -> string throws root.errors.Io {
    $rust_io_function
  }

  function bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }

  function seek(self, offset: int) -> null throws root.errors.Io {
    $rust_io_function
  }

  function write(self, data: string) -> int throws root.errors.Io {
    $rust_io_function
  }

  function write_bytes(self, data: uint8array) -> int throws root.errors.Io {
    $rust_io_function
  }
}

function file(path: string, mode: "r" | "r+") -> File throws root.errors.Io {
  $rust_io_function
}

function write(path: string, data: string) -> int throws root.errors.Io {
  $rust_io_function
}

function write_bytes(path: string, data: uint8array) -> int throws root.errors.Io {
  $rust_io_function
}
