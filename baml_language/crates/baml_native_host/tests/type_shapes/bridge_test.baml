// The native bridge tests' own functions, compiled beside the type-shapes
// fixture as the namespace `bridge_test`.

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
