/// List normalized file paths in an archive. Rejects absolute/traversing paths,
/// symlinks and hard links, before callers extract or activate any files.
function entries(data: uint8array, format: string) -> string[] throws root.errors.Io {
    $rust_io_function
}

/// Extract regular files/directories after validating every archive entry.
/// `format` is `"zip"` or `"tar.gz"`. Links and paths escaping `destination`
/// are rejected. The destination must be a fresh, caller-owned directory.
function extract(
    data: uint8array,
    format: string,
    destination: string,
) -> null throws root.errors.Io {
    $rust_io_function
}
