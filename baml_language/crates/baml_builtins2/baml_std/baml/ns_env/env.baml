/// Returns the value of the environment variable `key`, or `null` if not set.
///
/// Throws `ParseError` if the variable is set but its value is not valid
/// UTF-8 (the native bridge surfaces `std::env::VarError::NotUnicode` as a
/// catchable parse error).
function get(key: string) -> string? throws root.errors.Io | root.errors.ParseError {
  $rust_io_function
}

/// Returns the value of the environment variable `key`. Panics if not set.
function get_or_panic(key: string) -> string {
  get(key) ?? root.sys.panic("env var not found: " + key)
}
