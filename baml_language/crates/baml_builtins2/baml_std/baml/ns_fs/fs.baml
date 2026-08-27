/// A handle to an open file. Use `baml.fs.open` to obtain one.
///
/// Read and write operations on a closed handle throw `Io`. `text()`
/// additionally throws `ParseError` when the remaining bytes are not valid
/// UTF-8.
class File {
    _handle: $rust_type,

    /// Reads the entire remaining file contents as a UTF-8 string.
    ///
    /// # Throws
    ///
    /// - `Io` if an I/O error occurs while reading (for example, the file is closed).
    /// - `ParseError` if the remaining bytes are not valid UTF-8.
    function text(self) -> string throws root.errors.Io | root.errors.ParseError {
        self.as<root.io.Read>.text()
    }

    /// Reads the entire remaining file contents as raw bytes.
    ///
    /// # Throws
    ///
    /// - `Io` if an I/O error occurs while reading (for example, the file is closed).
    function bytes(self) -> uint8array throws root.errors.Io {
        self.as<root.io.Read>.bytes()
    }

    implements root.io.Read {
        /// Reads up to `limit` bytes from the file.
        /// Returns `null` if the end of the file has already been reached.
        ///
        /// # Throws
        ///
        /// - `Io` if an I/O error occurs while reading (for example, the file is closed).
        function read(self, limit: int) -> uint8array? throws root.errors.Io {
            $rust_io_function
        }

        // // reimplemented for perf reasons
        // function bytes(self) -> uint8array throws root.errors.Io {
        //     $rust_io_function
        // }
    }

    /// Closes the file handle, flushing any pending writes.
    /// Closing is an idempotent operation: calling `close()` multiple times is safe.
    ///
    /// # Throws
    ///
    /// - `Io` if an I/O error occurs while closing.
    function close(self) -> null throws root.errors.Io {
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

    implements root.io.Write {
        function write_some(self, data: uint8array) -> int throws root.errors.Io {
            $rust_io_function
        }

        function flush(self) -> void throws root.errors.Io {
            $rust_io_function
        }
    }

    /// Writes all of `data` to the file at the current cursor position.
    /// Returns the encoded byte count.
    function write(self, data: string | uint8array) -> int throws root.errors.Io {
        self.as<root.io.Write>.write(data)
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

/// Changes the permissions of the file at `path` to `mode`.
///
/// `mode` is the POSIX (unix) mode octal permissions value:
/// a numeric bitmask typically using three octal digits `0oUGO` (permissions for user/group/other)
/// Where each octal digit represents bit flags:
/// - `0o4` = read
/// - `0o2` = write
/// - `0o1` = execute
///
/// An optional fourth leading digit carries the setuid (`0o4000`), setgid
/// (`0o2000`), and sticky (`0o1000`) bits on platforms where these are supported.
/// `mode` must therefore be in `0 ..= 0o7777`; anything outside that range throws
/// `InvalidArgument` rather than being silently masked down to it.
///
/// On Windows there is no mode as a file is either writable or read-only.
/// Only the owner-write bit (`0o200`) has any effect: setting it clears the
/// read-only attribute and clearing it sets it. Every other bit, including the
/// group and other digits, is accepted and ignored. `path` must exist on
/// Windows even when the resulting attribute is unchanged.
function chmod(
    path: string,
    mode: int,
) -> void throws root.errors.Io | root.errors.InvalidArgument {
    $rust_io_function
}

/// Creates a symbolic link at `path` that points to `target`.
///
/// `target` is stored verbatim and need not exist: a dangling link is created
/// if it does not. A relative `target` is resolved by the operating system
/// against the directory containing `path`, not against the working directory.
///
/// Throws `Io` if `path` already exists.
///
/// On Windows the link is created as a directory link when `target` resolves to
/// an existing directory and as a file link otherwise (a dangling link is
/// always a file link), and creating one requires Developer Mode or
/// `SeCreateSymbolicLinkPrivilege`; without it the OS refuses and this throws
/// `Io`.
function symlink(target: string, path: string) -> void throws root.errors.Io {
    $rust_io_function
}
