function get(key: string) -> string? throws root.errors.Io {
  $rust_io_function
}

function get_or_panic(key: string) -> string {
  let val = get(key);
  if (val == null) {
    root.sys.panic("env var not found: " + key);
    "" // dummy code (sys.panic terminates)
  } else {
    val
  }
}
