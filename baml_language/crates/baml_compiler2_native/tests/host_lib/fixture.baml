// What `tests/host_lib.rs` calls through the library form.

function add(a: int, b: int) -> int {
  a + b
}

function seven() -> int {
  7
}

// A part of a long string is a slice, whose tag is not a string's.
function tail(s: string) -> string {
  s.slice(1, s.length())
}

function length_of(s: string) -> int {
  s.length()
}

// Nothing is left to release the hold the second `acquire` waits for.
function stuck() -> int {
  let s = baml.sync.Semaphore.new(1);
  let p = s.acquire();
  let q = s.acquire();
  1
}

function boom(n: int) -> int {
  if (n > 0) {
    throw "boom";
  }
  n
}

// Starts a task that fails, and never awaits it.
function fails_unawaited() -> int {
  let t = spawn { boom(1) };
  1
}

// Starts a task that fails after the call has returned.
function fails_after_return() -> int {
  let t = spawn {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(20));
    boom(2)
  };
  2
}

// Awaits a task that fails, and catches its error.
function fails_awaited() -> int {
  let t = spawn { boom(3) };
  (await t) catch (e) {
    _ => 30,
  }
}

// `baml.sys.exit` panics with `baml.panics.Exit`; in a library it never
// ends the host's process.
function quits(code: int) -> int {
  baml.sys.exit(code)
}

function quits_in_a_task(code: int) -> int {
  let t = spawn { quits(code) };
  5
}

function boom_export(n: int) -> int {
  boom(n)
}

function greet_default(name: string = "you") -> string {
  "hi " + name
}
