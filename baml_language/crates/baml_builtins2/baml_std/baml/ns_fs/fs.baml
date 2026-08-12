/// A handle to an open file. Use `baml.fs.open` to obtain one.
///
/// Every `File.*` method can throw `InvalidArgument` when the underlying
/// handle has already been closed (e.g. by a prior `close()`); read-string
/// methods can additionally throw `ParseError` on non-UTF-8 input.
class File {
    _handle: $rust_type,

    /// Reads the entire remaining file contents as a UTF-8 string.
    function text(
        self,
    ) -> string throws root.errors.Io | root.errors.InvalidArgument | root.errors.ParseError {
        $rust_io_function
    }

    /// Reads the entire remaining file contents as raw bytes.
    function bytes(self) -> uint8array throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Reads up to `n` bytes from the file and returns them as a UTF-8 string.
    function read(
        self,
        n: int,
    ) -> string throws root.errors.Io | root.errors.InvalidArgument | root.errors.ParseError {
        $rust_io_function
    }

    /// Reads up to `n` bytes from the file and returns them as raw bytes.
    function read_bytes(
        self,
        n: int,
    ) -> uint8array throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Closes the file handle, flushing any pending writes.
    function close(self) -> null throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Moves the file cursor. `whence` is `"start"`, `"current"`, or `"end"`. Returns the new cursor position in bytes.
    ///
    /// Throws `InvalidArgument` if `offset` is negative when paired with
    /// `whence="start"` (the underlying syscall takes an unsigned offset).
    function seek_from(
        self,
        whence: "start" | "current" | "end",
        offset: int,
    ) -> int throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Writes `data` to the file at the current cursor position. Returns the number of bytes written.
    function write(self, data: string) -> int throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Writes raw bytes `data` to the file at the current cursor position. Returns the number of bytes written.
    function write_bytes(
        self,
        data: uint8array,
    ) -> int throws root.errors.Io | root.errors.InvalidArgument {
        $rust_io_function
    }
}

/// Opens the file at `path` with the given `mode`.
///
/// Mode values: `"r"` (read), `"r+"` (read/write), `"w"` (write/truncate),
/// `"w+"` (read/write/truncate), `"a"` (append), `"a+"` (read/append).
function open(
    path: string,
    mode: "r" | "r+" | "w" | "w+" | "a" | "a+",
) -> File throws root.errors.Io | root.errors.InvalidArgument {
    $rust_io_function
}

/// Returns `true` if a file or directory exists at `path`.
function exists(path: string) -> bool throws root.errors.Io {
    $rust_io_function
}

/// Removes the file at `path`. Throws `Io` if the file does not exist or cannot be deleted.
///
/// This handles regular files only. To delete a directory use `remove_dir`
/// (empty directories) or `remove_dir_all` (directory trees).
function remove(path: string) -> null throws root.errors.Io {
    $rust_io_function
}

/// Returns the size of the file at `path` in bytes.
function size(path: string) -> int throws root.errors.Io {
    $rust_io_function
}

/// Reads the entire contents of the file at `path` as a UTF-8 string.
/// Throws `ParseError` if the file's bytes are not valid UTF-8.
function read(path: string) -> string throws root.errors.Io | root.errors.ParseError {
    $rust_io_function
}

/// Writes `content` to the file at `path`, creating or truncating it. Returns the number of bytes written.
function write(path: string, content: string) -> int throws root.errors.Io {
    $rust_io_function
}

/// Writes raw bytes `content` to the file at `path`, creating or truncating it. Returns the number of bytes written.
function write_bytes(path: string, content: uint8array) -> int throws root.errors.Io {
    $rust_io_function
}

/// A single entry returned by `read_dir`.
class DirEntry {
    name: string,
    is_dir: bool,
    is_file: bool,
    is_symlink: bool,
}

/// Options for `mkdir`.
class MkdirOptions {
    /// If `true`, creates all missing parent directories (like `mkdir -p`).
    recursive: bool,
}

/// Returns an array of directory entries for the directory at `path`.
function read_dir(path: string) -> DirEntry[] throws root.errors.Io {
    $rust_io_function
}

/// Creates the directory at `path`. Pass `MkdirOptions { recursive: true }` to create parent directories.
function mkdir(path: string, options: MkdirOptions) -> null throws root.errors.Io {
    $rust_io_function
}

/// Removes the empty directory at `path`. Throws `Io` if `path` is not a
/// directory, is not empty, or does not exist. Mirrors Bun's `fs.promises.rmdir`.
function remove_dir(path: string) -> null throws root.errors.Io {
    $rust_io_function
}

/// Recursively removes the directory at `path` and all of its contents.
/// Idempotent: returns successfully if `path` does not exist. Like Bun's
/// `fs.promises.rm(path, { recursive: true, force: true })` for directory trees
/// and missing paths — but, unlike `rm -rf`, it targets directories only and
/// throws `Io` if `path` is a regular file.
function remove_dir_all(path: string) -> null throws root.errors.Io {
    $rust_io_function
}
