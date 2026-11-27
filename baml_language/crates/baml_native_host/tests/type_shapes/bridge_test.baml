// The native bridge tests' own functions, compiled beside the type-shapes
// fixture as the namespace `bridge_test`. The one copy: the Rust tests
// include it, and the Python and Node staging scripts copy it into their
// staged fixture. The first part is what the Rust bindings export; the
// later parts were added for the Node and Python tests.

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

// Added after the review of the Node bridge.

// Starts a task that reads the environment two thousand times, and returns
// at once: I/O in flight while Node's process exits.
function spawn_reader() -> int {
  let t = spawn { read_env(2000) };
  1
}

function read_env(n: int) -> int {
  let found = 0;
  for (let i = 0; i < n; i += 1) {
    if (baml.env.get("PATH") != null) { found += 1; }
  }
  found
}

class Chain {
  value int
  next Chain?
}

function make_chain(n: int) -> Chain {
  let c = Chain { value: 0, next: null };
  for (let i = 1; i < n; i += 1) {
    c = Chain { value: i, next: c };
  }
  c
}

function round_trip_chain(c: Chain) -> Chain {
  c
}

// The same fields as Point, another class.
class Pair {
  x int
  y int
}

function round_trip_point(p: Point) -> Point {
  p
}

enum Mood {
  Happy
  Sad
}

class Note {
  text string
  meta string?
}

class Tag {
  text string
}

// Which member of a union an argument became.
function mood_or_text(v: Mood | string) -> string {
  match (v) {
    let m: Mood => "enum",
    let t: string => "string",
  }
}

function float_or_literal(v: float | 1 | 2) -> string {
  match (v) {
    let f: float => "float",
    let i: int => "int",
  }
}

function point_or_map(v: Point | map<string, int>) -> string {
  match (v) {
    let p: Point => "class",
    let m: map<string, int> => "map",
  }
}

function note_or_tag(v: Note | Tag) -> string {
  match (v) {
    let n: Note => "note",
    let t: Tag => "tag",
  }
}

function count_map(m: map<string, int>) -> int {
  m.length()
}

// Added for the Python tests.

// `string` first: an enum member is still the enum.
function text_or_mood(v: string | Mood) -> string {
  match (v) {
    let t: string => "string",
    let m: Mood => "enum",
  }
}

// Its elements decide which list type a list is, both ways. (A type test
// between array types does not compile natively yet: no `match` here.)
function round_trip_list_union(v: int[] | (int | string)[]) -> int[] | (int | string)[] {
  v
}

// The same variant names as `enums.Sentiment`, another enum.
enum Polarity {
  Positive
  Negative
}

function round_trip_polarity(p: Polarity) -> Polarity {
  p
}

function quit(code: int) -> int {
  baml.sys.exit(code)
}

// Starts a task that sleeps, then fails with nothing awaiting it, and
// returns at once.
function fail_later(milliseconds: int) -> int {
  let t = spawn { late_explode(milliseconds) };
  1
}

function late_explode(milliseconds: int) -> int {
  nap(milliseconds);
  explode(1)
}

// Added for the HTTP tests: requests the library sends itself.

class HttpReply {
  status int
  body string
}

// GET `url`. `timeout_ms` bounds the whole exchange.
function http_get(url: string, timeout_ms: int) -> HttpReply {
  let request = baml.http.Request.from_url(url)
    .set_method("GET")
    .set_timeout(baml.time.Duration.from_milliseconds(timeout_ms));
  let response = baml.http.send(request);
  HttpReply { status: response.status_code, body: response.text() }
}

class Greeting {
  name string
  count int
}

class Echo {
  name string
  count int
  served_by string
}

// POST `greeting` to `url` as JSON, and read the reply as an `Echo`.
function http_post_json(url: string, greeting: Greeting) -> Echo {
  let request = baml.http.Request.from_url(url)
    .set_method("POST")
    .set_headers({"content-type": "application/json"})
    .set_body(baml.json.stringify(greeting));
  let response = baml.http.send(request);
  baml.json.decode<Echo>(response.text())
}
