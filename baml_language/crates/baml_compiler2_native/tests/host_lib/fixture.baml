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

class Node {
  name string
  next Node?
}

// Throws an instance whose field is the instance itself.
function throw_loop() -> int {
  let n = Node { name: "loop", next: null };
  n.next = n;
  throw n;
}

// A sibling awaits the failed task after the call has returned: the
// failure has someone to take it, and is never reported unhandled.
function sibling_awaits() -> int {
  let t = spawn { boom(5) };
  let w = spawn {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(10));
    (await t) catch (e) {
      _ => 7,
    }
  };
  1
}

// A task fails, then the call can never finish.
function stuck_with_failure() -> int {
  let t = spawn { boom(6) };
  let s = baml.sync.Semaphore.new(1);
  let p = s.acquire();
  let q = s.acquire();
  1
}

function nap(milliseconds: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
  milliseconds
}

function env_or(name: string, fallback: string) -> string {
  let value = baml.env.get(name);
  if (value != null) { value } else { fallback }
}

function is_true(b: bool) -> bool {
  b
}

function takes_null(x: null) -> int {
  1
}

class Pair {
  a int[]
  b int[]
}

function pair_total(p: Pair) -> int {
  p.a.length() + p.b.length()
}

function two_lists(a: int[], b: int[]) -> int {
  a.length() + b.length()
}
