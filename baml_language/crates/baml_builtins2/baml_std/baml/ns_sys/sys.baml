class ProcessOptions {
  cwd string?
  env map<string, string>?
  timeout_ms int?
  stdin string?
}

class ShellOutput {
  stdout string
  stderr string
  exit_code int
  _output $rust_type

  function stdout_bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function stderr_bytes(self) -> uint8array throws root.errors.Io {
    $rust_io_function
  }

  function ok(self) -> bool {
    self.exit_code == 0
  }
}

function exec(program: string, args: string[]?, options: ProcessOptions?) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

function shell(command: string, options: ProcessOptions?) -> ShellOutput throws root.errors.Io | root.errors.Timeout {
  $rust_io_function
}

function sleep(ms: int) -> null throws root.errors.Io {
  $rust_io_function
}

function panic(message: string) -> never {
  $rust_function
}

function now_ms() -> int {
  $rust_function
}

//baml:mut_vm
function argv() -> string[] {
  $rust_function
}
