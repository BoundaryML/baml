function shell(command: string) -> string throws root.errors.Io {
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
