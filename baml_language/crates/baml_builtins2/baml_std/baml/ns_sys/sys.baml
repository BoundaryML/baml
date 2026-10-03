/// Text (UTF-8 encoded) or raw bytes to send to a process.
type ProcessInput = string | uint8array;

/// How a standard stream is connected to its parent.
type Stdio = "inherit" | "pipe" | "ignore";

/// (internal) Resolved arguments for the native process operations.
class _ProcessOptions {
    cwd: string?,
    /// Overlay the inherited environment; null removes a variable.
    env: map<string, string?>?,
    clear_env: bool,
    timeout: root.time.Duration?,
    input: ProcessInput?,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
    detached: bool,
}

/// A completed command. Uncaptured output streams contain empty bytes.
class ProcessOutput {
    stdout: uint8array,
    stderr: uint8array,
    exit_code: int,
    /// Unix signal number, when terminated by a signal.
    signal: int?,

    function ok(self) -> bool throws never {
        self.exit_code == 0 && self.signal == null
    }
}

/// The exit status of a subprocess.
class ProcessExit {
    /// Exit code, or 128 + signal number on Unix when terminated by a signal.
    exit_code: int,
    signal: int?,

    function ok(self) -> bool throws never {
        self.exit_code == 0 && self.signal == null
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
        type Item = string;
        type Error = root.errors.Io;

        function iter(
            self,
        ) -> root.iter.Iterator<Item = string, Error = root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = string;
        type Error = root.errors.Io;

        /// Wait for and return the next line, or `Done` at EOF.
        function next(self) -> string | root.iter.Done throws root.errors.Io {
            let line: string | root.iter.Done = root.iter.Done {};
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

/// A handle to an OS process. Releasing or collecting this handle does not
/// terminate the process and does not wait for it. Use `defer { child.kill() }`
/// when termination should follow a BAML scope's lifetime.
class Subprocess {
    _handle: $rust_type,
    pid: int,
    /// A stream is present only when its configuration is "pipe".
    stdin: WritePipe?,
    stdout: ReadPipe?,
    stderr: ReadPipe?,

    /// Wait for the child's exit status. The timeout applies to this call,
    /// without killing the child. A later call can wait again; completed
    /// processes return their status immediately, including repeated waits.
    function wait(
        self,
        timeout: root.time.Duration? = null,
    ) -> ProcessExit throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
        $rust_io_function
    }

    /// Request termination. Safe to call repeatedly or after the child exits.
    function kill(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Release the process handle without terminating or waiting for the child.
    /// Stream handles have independent lifetimes; close those explicitly if needed.
    function close(self) -> null throws never {
        $rust_io_function
    }

    /// Release the process handle; also invoked by GC. Termination stays explicit.
    function cleanup(self) -> void throws never {
        self.close();
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

    /// Close the write pipe, delivering EOF. Call flush() first if needed.
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

/// Execute a program directly and wait for its result. Arguments are passed
/// literally. Standard streams inherit by default; `capture_output = true`
/// collects both stdout and stderr. Nonzero exit codes are returned normally.
/// A timeout terminates this command; cancelling the call releases the child
/// handle without killing the process. Explicit `input` is written to a pipe.
function run(
    program: string,
    args: string[] = [],
    cwd: string? = null,
    env: map<string, string?>? = null,
    clear_env: bool = false,
    timeout: root.time.Duration? = null,
    input: ProcessInput? = null,
    stdin: Stdio = "inherit",
    stdout: Stdio = "inherit",
    stderr: Stdio = "inherit",
    capture_output: bool = false,
) -> ProcessOutput throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    let options = _ProcessOptions {
        cwd: cwd,
        env: env,
        clear_env: clear_env,
        timeout: timeout,
        input: input,
        stdin: stdin,
        stdout: stdout,
        stderr: stderr,
        detached: false,
    };
    if (capture_output) {
        options.stdout = "pipe";
        options.stderr = "pipe";
    }
    _run(program, args, options)
}

/// (internal) Direct command execution with resolved defaults.
function _run(
    program: string,
    args: string[],
    options: _ProcessOptions,
) -> ProcessOutput throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}

/// Interpret a command string in the host's shell and wait for its result.
/// Supports pipes, redirection and other shell syntax. Output and timeout
/// behavior are the same as `run`; this function always uses a shell.
function shell(
    command: string,
    cwd: string? = null,
    env: map<string, string?>? = null,
    clear_env: bool = false,
    timeout: root.time.Duration? = null,
    input: ProcessInput? = null,
    stdin: Stdio = "inherit",
    stdout: Stdio = "inherit",
    stderr: Stdio = "inherit",
    capture_output: bool = false,
) -> ProcessOutput throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    let options = _ProcessOptions {
        cwd: cwd,
        env: env,
        clear_env: clear_env,
        timeout: timeout,
        input: input,
        stdin: stdin,
        stdout: stdout,
        stderr: stderr,
        detached: false,
    };
    if (capture_output) {
        options.stdout = "pipe";
        options.stderr = "pipe";
    }
    _shell(command, options)
}

/// (internal) Shell execution with resolved defaults.
function _shell(
    command: string,
    options: _ProcessOptions,
) -> ProcessOutput throws root.errors.Io | root.errors.Timeout | root.errors.InvalidArgument {
    $rust_io_function
}

/// Start a process and return its handle immediately. Only "pipe" streams
/// are exposed on the handle; callers must drain piped outputs concurrently.
/// Dropping, closing or collecting the handle never kills or waits for the
/// process. Use `child.wait(timeout = ...)` to limit a wait without terminating it.
/// `detached = true` creates a new session on Unix or detaches from the
/// Windows console. It does not change GC behavior or stream configuration.
function subprocess(
    program: string,
    args: string[] = [],
    cwd: string? = null,
    env: map<string, string?>? = null,
    clear_env: bool = false,
    input: ProcessInput? = null,
    stdin: Stdio = "inherit",
    stdout: Stdio = "inherit",
    stderr: Stdio = "inherit",
    detached: bool = false,
) -> Subprocess throws root.errors.Io | root.errors.InvalidArgument {
    _subprocess(
        program,
        args,
        _ProcessOptions {
            cwd: cwd,
            env: env,
            clear_env: clear_env,
            timeout: null,
            input: input,
            stdin: stdin,
            stdout: stdout,
            stderr: stderr,
            detached: detached,
        },
    )
}

/// (internal) Subprocess creation with resolved defaults.
function _subprocess(
    program: string,
    args: string[],
    options: _ProcessOptions,
) -> Subprocess throws root.errors.Io | root.errors.InvalidArgument {
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
