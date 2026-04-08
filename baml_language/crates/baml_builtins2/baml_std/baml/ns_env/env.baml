function get(key: string) -> string? throws root.errors.Io {
  $rust_io_function
}

function get_or_panic(key: string) -> string {
  get(key) ?? root.sys.panic("env var not found: " + key)
}
