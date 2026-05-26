/// Returns the value of the environment variable `key`, or `null` if not set.
function get(key: string) -> string? throws root.errors.Io {
  $rust_io_function
}

/// Returns the value of the environment variable `key`. Panics if not set.
function get_or_panic(key: string) -> string {
  get(key) ?? root.sys.panic("env var not found: " + key)
}
