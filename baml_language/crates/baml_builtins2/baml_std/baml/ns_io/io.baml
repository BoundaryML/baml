/// Reads a line of text from stdin. If `prompt` is provided, it is printed first (without a newline).
function input(prompt: string?) -> string throws never {
    $rust_io_function
}

/// Write `s` to stdout with no trailing newline. Flushes immediately so the
/// bytes are visible before the next sysop or await suspends the thread.
function print(s: string) -> null throws never {
    $rust_io_function
}

/// Write `s` to stdout followed by a newline.
function println(s: string) -> null throws never {
    $rust_io_function
}

/// Like `print` but writes to stderr.
function eprint(s: string) -> null throws never {
    $rust_io_function
}

/// Like `println` but writes to stderr.
function eprintln(s: string) -> null throws never {
    $rust_io_function
}
