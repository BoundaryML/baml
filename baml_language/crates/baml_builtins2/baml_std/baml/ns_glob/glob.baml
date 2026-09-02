/// Options for `Glob.scan`.
class ScanOptions {
    /// Working directory to resolve relative patterns from.
    cwd: string?,
    /// Include dotfiles (hidden files starting with `.`).
    dot: bool?,
    /// Return absolute paths instead of relative paths.
    absolute: bool?,
    /// Follow symbolic links when scanning.
    follow_symlinks: bool?,
    /// Throw an error if a broken symbolic link is encountered.
    throw_error_on_broken_symlink: bool?,
    /// Only return files, not directories.
    only_files: bool?,
}

/// A compiled glob pattern. Create one with `baml.glob.new(pattern)`.
/// Patterns and paths use `/` separators on every platform, including
/// Windows drive paths (`C:/work/**`) and UNC paths (`//server/share/**`).
/// Backslash is the glob escape character, not a native path separator.
class Glob {
    _handle: $rust_type,

    /// Scans the filesystem and returns all paths matching this glob pattern.
    /// Pass a `string` root path or a `ScanOptions` object for more control.
    function scan(self, root: string | ScanOptions) -> string[] throws root.errors.Io {
        $rust_io_function
    }

    /// Returns `true` if `path` matches this glob pattern.
    /// `path` must use the portable `/`-separated form described above.
    function matches(self, path: string) -> bool throws root.errors.Io {
        $rust_io_function
    }
}

/// Compiles a portable, `/`-separated glob `pattern` and returns a `Glob` object.
/// `Glob.scan` accepts a native `cwd` but matches walker entries in portable form.
function new(pattern: string) -> Glob throws root.errors.Io {
    $rust_io_function
}
