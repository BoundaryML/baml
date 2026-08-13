/// Options for `exec`, `start_process`, and `shell`.
class ProcessOptions {
    /// Working directory for the child process.
    cwd: string?,
    /// Environment variables to set for the child process.
    env: map<string, string>?,
    /// Maximum time in milliseconds to wait for the process to complete.
    timeout_ms: int?,
    /// Data to write to the child process's stdin.
    stdin: string?,
    /// Keep a writable stdin pipe on a process returned by `start_process`.
    ///
    /// When this is `true`, use `Process.write_stdin` to send data incrementally
    /// and `Process.close_stdin` to deliver EOF. This option has no effect on
    /// `exec` or `shell`.
    keep_stdin_open: bool?,
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
    exit_code: int,
    signal: string?,

    /// Returns `true` if the process exited normally with code `0`.
    function ok(self) -> bool {
        self.exit_code == 0
    }
}

/// A live line-oriented child-process output stream.
///
/// Iteration waits for one complete UTF-8 line without blocking other BAML
/// green threads. The final unterminated line is yielded at EOF.
class ProcessLineStream {
    _handle: $rust_type,

    implements root.iter.Iterable {
        type Item = string
        type Error = root.errors.Io | root.errors.Timeout

        function iter(
            self,
        ) -> root.iter.Iterator<Item = string, Error = root.errors.Io | root.errors.Timeout> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = string
        type Error = root.errors.Io | root.errors.Timeout

        function next(self) -> string | root.iter.Done throws root.errors.Io | root.errors.Timeout {
            match (self._next()) {
                let line: string => line,
                null => root.iter.Done {  },
            }
        }
    }

    /// Wait for and return the next line, or `null` at EOF.
    function _next(self) -> string? throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Stop reading this stream. Later reads return EOF.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// A live child process created with `start_process`.
///
/// `stdout` is consumed incrementally. Stderr inherits the parent process so
/// diagnostics remain visible without risking a blocked, unread stderr pipe.
class Process {
    stdout: ProcessLineStream,
    _handle: $rust_type,

    /// Write data to the child process's open stdin pipe.
    function write_stdin(self, data: string) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Close the child process's stdin pipe, delivering EOF.
    function close_stdin(self) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Wait for the child to exit.
    function wait(self) -> ProcessExit throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Request that the child terminate.
    function kill(self) -> null throws root.errors.Io {
        $rust_io_function
    }

    /// Terminate an un-waited child and close its stdout stream.
    ///
    /// Safe to use with `defer { process.close() }`.
    function close(self) -> null throws never {
        $rust_io_function
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

/// Starts `program` and returns immediately with a live stdout line stream.
///
/// Unlike `exec`, output is not buffered until process exit. Reading
/// `process.stdout` suspends only the current BAML green thread.
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

/// Panics with `message`, throwing a `baml.panics.UserPanic`. Never returns.
//baml:fallible
function panic(message: string) -> never throws never {
    $rust_function
}

/// Terminates the process with the given exit `code`. Throws `baml.panics.Exit` which, if unhandled, exits the process.
//baml:fallible
function exit(code: int) -> never throws never {
    $rust_function
}

//baml:mut_vm
/// Returns the command-line arguments passed to the BAML program, as an array of strings.
function argv() -> string[] throws never {
    $rust_function
}
