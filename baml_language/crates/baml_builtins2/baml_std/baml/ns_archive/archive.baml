/// Extracts tar.gz or zip, rejecting path traversal and conflicting normalized paths; ignores archive links.
/// The destination must be an empty directory and cannot itself be a symlink.
/// The destination may be partially populated on failure; use a staging directory and rename to publish.
function extract(
    data: uint8array,
    format: "tar.gz" | "zip",
    destination: string,
) -> void throws root.errors.Io {
    $rust_io_function
}

/// Reads a named file without writing to disk; validates paths throughout the archive to reject ambiguity.
function read_file(
    data: uint8array,
    format: "tar.gz" | "zip",
    name: string,
) -> uint8array throws root.errors.Io {
    $rust_io_function
}
