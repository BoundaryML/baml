/// Options for `exec` and `shell`.
class ProcessOptions {
  /// Working directory for the child process.
  cwd string?
  /// Environment variables to set for the child process.
  env map<string, string>?
  /// Maximum time in milliseconds to wait for the process to complete.
  timeout_ms int?
  /// Data to write to the child process's stdin.
  stdin string?
}

/// The result of running an external process with `exec` or `shell`.
class ShellOutput {
  stdout uint8array
  stderr uint8array
  exit_code int

  /// Returns `true` if the process exited with code `0`.
  function ok(self) -> bool {
    self.exit_code == 0
  }
}

/// Runs `program` with the given `args` and returns its output. Throws on I/O failure or timeout.
function exec(program: string, args: string[]?, options: ProcessOptions?) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

/// Runs a shell `command` string (passed to `/bin/sh -c`) and returns its output.
function shell(command: string, options: ProcessOptions?) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

/// Suspends the current thread for `ms` milliseconds.
function sleep(ms: int) -> null throws root.errors.Io {
  $rust_io_function
}

/// Panics with `message`, throwing a `baml.panics.UserPanic`. Never returns.
function panic(message: string) -> never {
  $rust_function
}

/// Terminates the process with the given exit `code`. Throws `baml.panics.Exit` which, if unhandled, exits the process.
function exit(code: int) -> never {
  $rust_function
}

/// Returns the current time as milliseconds since the Unix epoch (UTC).
function now_ms() -> int {
  $rust_function
}

//baml:mut_vm
/// Returns the command-line arguments passed to the BAML program, as an array of strings.
function argv() -> string[] {
  $rust_function
}
