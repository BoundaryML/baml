/// Reads a line of text from stdin. If `prompt` is provided, it is printed first (without a newline).
///
/// The trailing newline is stripped; end-of-input reads as `""`.
///
/// Unlike the `print` family — which panics with `baml.panics.HostUnavailable`
/// on a dead output stream, as Rust's `println!` does — reading input is an
/// operation callers routinely want to handle, so a failed read is an error
/// value.
///
/// # Throws
///
/// - `Io` if the prompt cannot be written or stdin cannot be read.
function input(prompt: string?) -> string throws root.errors.Io {
    $rust_io_function
}

/// Write `s` to stdout with no trailing newline. Flushes immediately so the
/// bytes are visible before the next sysop or await suspends the thread.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if stdout cannot be written
function print(s: string) -> null throws never {
    $rust_io_function
}

/// Write `s` to stdout followed by a newline.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if stdout cannot be written
function println(s: string) -> null throws never {
    $rust_io_function
}

/// Like `print` but writes to stderr.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if stderr cannot be written
function eprint(s: string) -> null throws never {
    $rust_io_function
}

/// Like `println` but writes to stderr.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if stderr cannot be written
function eprintln(s: string) -> null throws never {
    $rust_io_function
}
