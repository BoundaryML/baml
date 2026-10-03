/// Extracts a tar.gz or zip archive, rejecting path traversal and ignoring archive links.
/// The destination must be an empty directory and cannot itself be a symlink.
/// The destination may be partially populated on failure; use a staging directory and rename to publish.
function extract(
    data: uint8array,
    format: "tar.gz" | "zip",
    destination: string,
) -> void throws root.errors.Io {
    $rust_io_function
}

/// Reads a named file from an archive without writing it to disk.
function read_file(
    data: uint8array,
    format: "tar.gz" | "zip",
    name: string,
) -> uint8array throws root.errors.Io {
    $rust_io_function
}
