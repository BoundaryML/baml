class ScanOptions {
  cwd string?
  dot bool?
  absolute bool?
  follow_symlinks bool?
  throw_error_on_broken_symlink bool?
  only_files bool?
}

class Glob {
  _handle $rust_type

  function scan(self, root: string | ScanOptions) -> string[] throws root.errors.Io {
    $rust_io_function
  }

  function matches(self, path: string) -> bool throws root.errors.Io {
    $rust_io_function
  }
}

function new(pattern: string) -> Glob throws root.errors.Io {
  $rust_io_function
}
