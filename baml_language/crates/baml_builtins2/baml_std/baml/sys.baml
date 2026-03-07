function shell(command: string) -> string throws baml.errors.Io {
  $rust_io_function
}

function sleep(ms: int) -> null throws baml.errors.Io {
  $rust_io_function
}

@@internal.panics(HostPanic)
function panic(message: string) -> null {
  $rust_io_function
}
