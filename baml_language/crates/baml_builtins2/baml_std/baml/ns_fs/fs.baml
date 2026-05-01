class File {
  _handle $rust_type

  function text(self) -> string throws root.errors.Io {
    $rust_io_function
  }

  function bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function read(self, n: int) -> string throws root.errors.Io {
    $rust_io_function
  }

  function read_bytes(self, n: int) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function close(self) -> null throws root.errors.Io {
    $rust_io_function
  }

  function seek_from(self, whence: "start" | "current" | "end", offset: int) -> int throws root.errors.Io {
    $rust_io_function
  }

  function write(self, data: string) -> int throws root.errors.Io {
    $rust_io_function
  }

  function write_bytes(self, data: uint8array) -> int throws root.errors.Io {
    $rust_io_function
  }
}

function open(path: string, mode: "r" | "r+" | "w" | "w+" | "a" | "a+") -> File throws root.errors.Io {
  $rust_io_function
}

function exists(path: string) -> bool throws root.errors.Io {
  $rust_io_function
}

function remove(path: string) -> null throws root.errors.Io {
  $rust_io_function
}

function size(path: string) -> int throws root.errors.Io {
  $rust_io_function
}

function read(path: string) -> string throws root.errors.Io {
  $rust_io_function
}

function write(path: string, content: string) -> int throws root.errors.Io {
  $rust_io_function
}

function write_bytes(path: string, content: uint8array) -> int throws root.errors.Io {
  $rust_io_function
}

class DirEntry {
  name string
  is_dir bool
  is_file bool
  is_symlink bool
}

class MkdirOptions {
  recursive bool
}

function read_dir(path: string) -> DirEntry[] throws root.errors.Io {
  $rust_io_function
}

function mkdir(path: string, options: MkdirOptions) -> null throws root.errors.Io {
  $rust_io_function
}
