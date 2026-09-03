/// Options for `exec`, `start_process`, and `shell`.
class ProcessOptions {
    /// Working directory for the child process.
    cwd: string?,
    /// Environment variables to set for the child process.
    env: map<string, string>?,
    /// Maximum time in milliseconds to wait for the process to complete.
    timeout_ms: int?,
    /// The child process's complete stdin. `start_process` writes it and then
    /// closes the pipe. Leave it unset to drive `Process.stdin` incrementally.
    stdin: string?,
    /// Where the child's stderr goes. `null` means `StderrMode.Inherit`, and
    /// only `StderrMode.Pipe` populates `Process.stderr`.
    stderr: StderrMode?,
}

/// Where a child process's standard error goes.
enum StderrMode {
    /// The child writes to this process's stderr. This is the default.
    Inherit,
    /// The child's stderr becomes `Process.stderr`. The caller must drain it
    /// concurrently with stdout to prevent the child from blocking.
    Pipe,
    /// The child's stderr is discarded.
    Discard,
}

/// The result of running an external process with `exec` or `shell`.
class ShellOutput {
    stdout: uint8array,
    stderr: uint8array,
    exit_code: int,

    /// Returns `true` if the process exited with code `0`.
    function ok(self) -> bool {
        self.exit_code == 0
    }
}

/// The terminal status of a process created with `start_process`.
class ProcessExit {
    /// The exit code of the process.
    exit_code: int,
    /// The signal that caused the process to exit, if any.
    signal: string?,

    /// Returns `true` if the process exited normally with code `0`.
    function ok(self) -> bool throws never {
        self.exit_code == 0
    }
}

/// A line-oriented view over a `ReadPipe`.
///
/// It reads through to the pipe on demand and holds only bytes past the last
/// newline. The final unterminated line is yielded at EOF. Invalid UTF-8 is
/// replaced with U+FFFD.
class ReadPipeLines {
    pipe: ReadPipe,
    _pending: uint8array,

    implements root.iter.Iterable {
        type Item = string
        type Error = root.errors.Io

        function iter(
            self,
        ) -> root.iter.Iterator<Item = string, Error = root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = string
        type Error = root.errors.Io

        /// Wait for and return the next line, or `Done` at EOF.
        function next(self) -> string | root.iter.Done throws root.errors.Io {
            let line: string | root.iter.Done = root.iter.Done {  };
            let framing = true;
            while (framing) {
                match (self._pending.index_of(10)) {
                    let index: int => {
                        let bytes = self._pending.slice(0, index);
                        self._pending = self._pending.slice(index + 1, self._pending.length());
                        line = _strip_carriage_return(bytes).to_string();
                        framing = false;
                    },
                    null => {
                        match (self.pipe.read(65536)) {
                            let chunk: uint8array => {
                                self._pending = self._pending.concat(chunk);
                            },
                            null => {
                                if (self._pending.length() > 0) {
                                    line = _strip_carriage_return(self._pending).to_string();
                                    self._pending = b"";
                                }
                                framing = false;
                            },
                        }
                    },
                }
            }
            line
        }
    }
}

/// (internal) Drop one trailing carriage return so CRLF input produces clean lines.
function _strip_carriage_return(line: uint8array) -> uint8array throws never {
    match (line.at(line.length() - 1)) {
        13 => line.slice(0, line.length() - 1),
        _ => line,
    }
}

/// A live child process created with `start_process`.
///
/// `stdin` and `stdout` are always caller-owned pipes. `stderr` is populated
/// only when `ProcessOptions.stderr` is `StderrMode.Pipe`.
class Process {
    _handle: $rust_type,
    stdin: WritePipe,
    stdout: ReadPipe,
    stderr: ReadPipe?,

    /// Wait for the child to exit.
    function wait(self) -> ProcessExit throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Request that the child terminate.
    function kill(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Terminate an un-waited child and close its stdin, stdout, and stderr pipes.
    ///
    /// Safe to use with `defer { process.close() }`.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// A system pipe for reading data, typically from another process.
class ReadPipe {
    _pipe: $rust_type,

    /// Close the read pipe, releasing any resources.
    function close(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Return a line-oriented view over this pipe.
    function lines(self) -> ReadPipeLines throws never {
        ReadPipeLines { pipe: self, _pending: b"" }
    }

    implements root.io.Read {
        function read(self, limit: int) -> uint8array? throws root.errors.Io {
            $rust_io_function
        }
    }
}

/// A system pipe for writing data, typically to another process.
class WritePipe {
    _pipe: $rust_type,

    /// Close the write pipe, flushing pending data and delivering EOF.
    function close(self) -> void throws root.errors.Io {
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
}

/// Runs `program` with the given `args` and returns its output. Throws on I/O failure or timeout.
function exec(
    program: string,
    args: string[]?,
    options: ProcessOptions?,
) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}

/// Starts `program` and returns a live process with caller-owned pipes.
///
/// Unlike `exec`, output is not buffered until process exit. Reading
/// `process.stdout` suspends only the current BAML green thread. If
/// `ProcessOptions.stdin` is set, this call writes that input and waits for the
/// child to consume it, bounded by `ProcessOptions.timeout_ms`.
function start_process(
    program: string,
    args: string[]?,
    options: ProcessOptions?,
) -> Process throws root.errors.Io {
    $rust_io_function
}

/// Runs a shell `command` string (passed to `/bin/sh -c`) and returns its output.
function shell(
    command: string,
    options: ProcessOptions?,
) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
    $rust_io_function
}

/// Suspends the current thread for `delay`.
function sleep(delay: root.time.Duration) -> null throws root.errors.Io {
    $rust_io_function
}

/// Forces a full garbage collection and runs queued `cleanup()` finalizers before returning.
/// Intended for deterministic tests and runtime diagnostics; production code should normally
/// let the runtime schedule collections automatically.
function collect_garbage() -> null throws never {
    $rust_io_function
}

/// Panics with `message`.
///
/// # Panics
///
/// - Always panics with `baml.panics.UserPanic`.
//baml:fallible
function panic(message: string) -> never throws never {
    throw root.panics.UserPanic { message };
}

/// Terminates the process with the given exit `code`.
///
/// # Panics
///
/// - Always panics with `baml.panics.Exit` which, if unhandled, exits the process.
//baml:fallible
function exit(code: int) -> never throws never {
    throw root.panics.Exit { code };
}

//baml:mut_vm
/// Returns the command-line arguments passed to the BAML program, as an array of strings.
function argv() -> string[] throws never {
    $rust_function
}

/// Returns the process ID of the current process.
///
/// # Panics
///
/// - `baml.panics.HostUnavailable` if the environment does not support process IDs (e.g. in a browser).
function pid() -> int throws never {
    $rust_io_function
}
