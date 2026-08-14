/// Options for `exec`, `start_process`, and `shell`.
class ProcessOptions {
    /// Working directory for the child process.
    cwd: string?,
    /// Environment variables to set for the child process.
    env: map<string, string>?,
    /// Maximum time in milliseconds to wait for the process to complete.
    timeout_ms: int?,
    /// The child process's complete stdin. `start_process` writes it and then
    /// closes the pipe, so a child that reads to EOF sees one. Leave it unset
    /// to drive `Process.stdin` incrementally instead.
    stdin: string?,
    /// No longer has any effect: `start_process` always hands back a writable
    /// `Process.stdin`, and EOF is delivered by closing that pipe.
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

/// A line-oriented view over a `ReadPipe`.
///
/// A combinator, not a resource: it reads through to the pipe on demand and
/// holds only the bytes past the last newline, so the pipe stays open and
/// owned by whoever created it. Interleaving raw `pipe.read` calls with line
/// reads splits a line across the two — a caller error the type does not
/// prevent.
///
/// Iteration waits for one complete UTF-8 line without blocking other BAML
/// green threads. The final unterminated line is yielded at EOF, and invalid
/// UTF-8 is replaced with U+FFFD rather than throwing.
class ReadPipeLines {
    /// The pipe being framed.
    pipe: ReadPipe,
    /// Bytes read past the last newline, carried into the next `next()`.
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
            // `line` stays `Done` until a newline is found or EOF flushes the
            // tail; `framing` ends the loop in both cases.
            let line: string | root.iter.Done = root.iter.Done {  };
            let framing = true;
            while (framing) {
                // 10 is `\n`; 65536 is how much one framing read asks for.
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
                            // EOF: whatever follows the last newline is the
                            // final, unterminated line; an empty remainder is
                            // just the end.
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

/// Drops one trailing `\r` (13) so CRLF input frames into clean lines.
function _strip_carriage_return(line: uint8array) -> uint8array throws never {
    match (line.at(line.length() - 1)) {
        13 => line.slice(0, line.length() - 1),
        _ => line,
    }
}

/// A live child process created with `start_process`.
///
/// All three streams are pipes the caller owns and consumes incrementally.
/// Stderr is no longer inherited from the parent, so a child that writes more
/// than the pipe buffer holds blocks until `stderr` is drained.
class Process {
    _handle: $rust_type,
    stdin: WritePipe,
    stdout: ReadPipe,
    stderr: ReadPipe,

    /// Wait for the child to exit.
    function wait(self) -> ProcessExit throws root.errors.Io | root.errors.Timeout {
        $rust_io_function
    }

    /// Request that the child terminate.
    function kill(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// Terminate an un-waited child and close its stdout stream.
    ///
    /// Safe to use with `defer { process.close() }`.
    function close(self) -> null throws never {
        $rust_io_function
    }
}

/// A system pipe for reading data, typically from another process.
///
/// Generally should only be used with a single reader, competing `read` calls
/// will behave FIFO.
class ReadPipe {
    _pipe: $rust_type,

    /// Close the read pipe, releasing any resources.
    function close(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    /// A line-oriented view over this pipe. The pipe stays open and readable;
    /// the stream only buffers what it has read past the last newline.
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

    /// Close the write pipe, releasing any resources.
    function close(self) -> void throws root.errors.Io {
        $rust_io_function
    }

    implements root.io.Write {
        function write(self, data: uint8array) -> int throws root.errors.Io {
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

/// Returns the process ID of the current process.
///
/// ## Panics
/// If the environment does not support process IDs (e.g. in a browser), with
/// `baml.panics.HostUnavailable`.
function pid() -> int throws never {
    $rust_io_function
}
