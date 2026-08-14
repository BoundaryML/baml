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

/// Like `get_or_panic`, but tolerant of a missing variable when `lenient` is
/// true: a missing variable yields the empty string instead of panicking.
///
/// Used by generated client constructors so that offline prompt rendering
/// (`<Fn>$render_prompt`) can build a client for its provider/role metadata
/// without requiring credentials. The network path (`<Fn>$call`) constructs
/// with `lenient = false`, so a missing `api_key` env var still panics for it,
/// exactly as before.
function get_or_panic_lenient(key: string, lenient: bool) -> string {
    if (lenient) {
        get(key) ?? ""
    } else {
        get_or_panic(key)
    }
}
