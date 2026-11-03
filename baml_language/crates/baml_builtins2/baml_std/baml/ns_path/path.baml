/// Joins path components using the host's path rules. An absolute component resets the preceding path.
function join(parts: string[]) -> string throws never {
    $rust_io_function
}

/// Returns the parent directory, or null at a root.
function parent(path: string) -> string? throws never {
    $rust_io_function
}

/// Whether the path is absolute on this host.
function is_absolute(path: string) -> bool throws never {
    $rust_io_function
}

/// Expands a leading ~/ and resolves relative paths against base (or the program's working directory).
/// The path does not have to exist. A relative base is resolved against the working directory.
/// Uses the host's absolute-path rules without resolving symlinks. On Unix, `..`
/// is retained so paths through symlinks keep their meaning, even for new files.
function absolute(path: string, base: string? = null) -> string throws root.errors.Io {
    _absolute(path, base)
}

/// (internal) Native path resolution.
function _absolute(path: string, base: string?) -> string throws root.errors.Io {
    $rust_io_function
}
