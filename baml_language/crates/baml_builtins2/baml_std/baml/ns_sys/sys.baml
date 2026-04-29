class ShellOutput {
  stdout string
  stderr string
  exit_code int

  function ok(self) -> bool {
    self.exit_code == 0
  }
}

function shell(command: string) -> ShellOutput throws root.errors.Io {
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
