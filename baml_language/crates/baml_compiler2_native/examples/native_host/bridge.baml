// What the native bridges spike calls from a host program: see
// `examples/native_host.rs`.

function add(a: int, b: int) -> int {
  a + b
}

// Parks its task on the worker's timer: the call ends in a later turn.
function nap(milliseconds: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
  milliseconds
}

// Parks its task on the I/O thread: the call ends after a wake from it.
function env_or(name: string, fallback: string) -> string {
  let value = baml.env.get(name);
  if (value != null) { value } else { fallback }
}

function greet(name: string) -> string {
  "hello, " + name + "!"
}

function fail(message: string) -> int {
  throw message;
}
