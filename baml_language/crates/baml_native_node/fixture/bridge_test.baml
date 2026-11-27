// The native bridge tests' own functions, compiled beside the type-shapes
// fixture as the namespace `bridge_test`. A copy of
// `baml_native_host/tests/type_shapes/bridge_test.baml`, so that both
// sides of the Node bench (today's SDK and the native bindings) run them.

class LookupFailed {
  key string
  code int
}

function lookup(key: string) -> string throws LookupFailed {
  if (key == "missing") {
    throw LookupFailed { key: key, code: 404 };
  }
  key
}

class Point {
  x int
  y int
}

function add(a: int, b: int) -> int {
  a + b
}

function round_trip_points(points: Point[]) -> Point[] {
  points
}

function make_points(n: int) -> Point[] {
  let points: Point[] = [];
  for (let i = 0; i < n; i += 1) {
    points.push(Point { x: i, y: n - i });
  }
  points
}

function count_points(points: Point[]) -> int {
  points.length()
}

function make_ints(n: int) -> int[] {
  let xs: int[] = [];
  for (let i = 0; i < n; i += 1) {
    xs.push(i);
  }
  xs
}

function count_ints(xs: int[]) -> int {
  xs.length()
}

// Parks on the worker's timer.
function nap(milliseconds: int) -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(milliseconds));
  milliseconds
}

// Parks on the I/O thread.
function env_or(name: string, fallback: string) -> string {
  let value = baml.env.get(name);
  if (value != null) { value } else { fallback }
}

function two_lists(a: int[], b: int[]) -> int {
  a.length() + b.length()
}

function explode(n: int) -> int {
  if (n > 0) {
    throw "exploded";
  }
  n
}

// Starts a task that fails, and never awaits it.
function fail_quietly() -> int {
  let t = spawn { explode(1) };
  1
}

// Added for the Node tests.

// Nothing is left to release the hold the second `acquire` waits for: the
// native library reports the call stuck.
function stuck() -> int {
  let s = baml.sync.Semaphore.new(1);
  let p = s.acquire();
  let q = s.acquire();
  1
}

// Starts a task that sleeps ten seconds, and returns at once: a BAML timer
// with no call pending, which must not keep Node's process alive.
function spawn_sleeper() -> int {
  let t = spawn { nap(10000) };
  1
}

function divide(a: int, b: int) -> int {
  a / b
}
