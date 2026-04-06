function shell(command: string) -> string throws root.errors.Io {
  $rust_io_function
}

function sleep(ms: int) -> null throws root.errors.Io {
  $rust_io_function
}

function panic(message: string) -> null {
  $rust_io_function
}

function now_ms() -> int {
  $rust_function
}
