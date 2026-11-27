// Values whose type is only known when the program runs: an optional, a
// union, an `unknown`, an interface value. Each is held as a tag and a
// payload wherever it lives, with no box around an `int`, a `float` or a
// `bool`.

class ValThing { size: int }
class ValError { code: int }
enum ValColor {
  Red
  Green
  Blue
}

function val_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function val_maybe(n: int) -> int? {
  if n % 2 == 0 { n } else { null }
}
function val_either(i: int) -> int | string {
  if i % 2 == 0 { i } else { rep("s", i) }
}
function val_three(i: int) -> int | float | bool {
  if i % 3 == 0 { i } else if i % 3 == 1 { halves(i) } else { i > 3 }
}
// What only a static type could tell apart, held where nothing says it.
function val_hide(value: unknown, n: int) -> unknown {
  let held: unknown[] = [];
  for (let i = 0; i < n; i += 1) { held.push(value); };
  held[n - 1]
}

// An optional `int`, `float` or `bool` is one element of an array.
function val_optional_ints() -> string {
  let held: int?[] = [];
  for (let i = 0; i < 6; i += 1) { held.push(val_maybe(i * 5)); };
  let out = `${held.length()}`;
  for (let x in held) { out = out + ` ${x ?? -1}`; };
  out
}
function val_optional_floats_and_bools() -> string {
  let floats: float?[] = [halves(1), null, halves(3)];
  let bools: bool?[] = [val_count(1) == 1, null, val_count(1) == 2];
  let out = "";
  for (let x in floats) { out = out + `${x ?? 9.5} `; };
  for (let b in bools) {
    out = out + if b == null { "none " } else if b ?? false { "yes " } else { "no " };
  };
  out
}
function val_optional_written_in_place() -> string {
  let held: int?[] = [null, null, null];
  held[val_count(1)] = val_count(7);
  held[0] = held[1];
  held[1] = null;
  `${held[0] ?? -1} ${held[1] ?? -1} ${held[2] ?? -1}`
}
// And one value of a map, where `null` is a value like any other.
function val_optional_map_values() -> string {
  let m: map<string, int?> = {};
  m[rep("a", 1)] = val_maybe(4);
  m[rep("b", 1)] = val_maybe(5);
  m[rep("c", 1)] = val_count(0);
  `${m["a"] ?? -1} ${m["b"] ?? -1} ${m["c"] ?? -1} ${m.length()} ${m.has("b")} ${m.has("d")}`
}
function val_null_map_values() -> string {
  let seen: map<int, null> = {};
  for (let i = 0; i < 5; i += 1) { seen[val_count(i) % 3] = null; };
  `${seen.length()} ${seen.has(2)} ${seen.has(3)} ${seen[1] == null}`
}
// And the value of a future.
function val_optional_future() -> string {
  let some = spawn { val_maybe(val_count(4)) };
  let none = spawn { val_maybe(val_count(5)) };
  let half = spawn { if val_count(1) == 1 { halves(3) } else { null } };
  `${(await some) ?? -1} ${(await none) ?? -1} ${(await half) ?? 0.0}`
}
function val_union_future() -> string {
  let odd = spawn { val_either(val_count(3)) };
  let even = spawn { val_either(val_count(4)) };
  let color = spawn { if val_count(1) == 1 { ValColor.Blue } else { ValColor.Red } };
  let text = match (await odd) {
    let n: int => `int ${n}`,
    let s: string => `string ${s}`,
  };
  let number = match (await even) {
    let n: int => `int ${n}`,
    let s: string => `string ${s}`,
  };
  `${text} ${number} ${(await color) == ValColor.Blue}`
}

// A union of scalars in an array and in a map, read back by `match`.
function val_union_array() -> string {
  let held: (int | string)[] = [];
  for (let i = 0; i < 5; i += 1) { held.push(val_either(i)); };
  let out = "";
  for (let x in held) {
    out = out + match (x) {
      let n: int => `int ${n + 1}`,
      let s: string => `string ${s}`,
    } + ",";
  };
  out
}
function val_union_map() -> string {
  let held: map<string, int | float | bool> = {};
  for (let i = 0; i < 6; i += 1) { held[rep("k", i + 1)] = val_three(i); };
  let out = "";
  for (let i = 0; i < 6; i += 1) {
    out = out + match (held[rep("k", i + 1)]) {
      let n: int => `i${n}`,
      let f: float => `f${f}`,
      let b: bool => `b${b}`,
    } + " ";
  };
  out
}
function val_union_field() -> string {
  let things: ValHolder[] = [];
  for (let i = 0; i < 4; i += 1) { things.push(ValHolder { held: val_three(i), maybe: val_maybe(i), shade: if i == 2 { ValColor.Green } else { null } }); };
  let out = "";
  for (let thing in things) {
    out = out + match (thing.held) {
      let n: int => `i${n}`,
      let f: float => `f${f}`,
      let b: bool => `b${b}`,
    } + `/${thing.maybe ?? -1}/${thing.shade == null} `;
  };
  out
}
class ValHolder {
  held: int | float | bool
  maybe: int?
  shade: ValColor?
}

// An interface value that is an `int` or a `string` is dispatched on like
// one that is an instance.
interface ValNamed {
  function named(self) -> string throws never
}
implements ValNamed for int {
  function named(self) -> string throws never { `int ${self + 1}` }
}
implements ValNamed for string {
  function named(self) -> string throws never { `string ${self.length()}` }
}
implements ValNamed for ValThing {
  function named(self) -> string throws never { `thing ${self.size}` }
}
function val_pick(i: int) -> ValNamed {
  if i == 0 { val_count(41) } else if i == 1 { rep("ab", 2) } else { ValThing { size: i } }
}
function val_dispatch_on_scalars() -> string {
  let out = "";
  for (let i = 0; i < 3; i += 1) { out = out + val_pick(i).named() + ","; };
  out
}
function val_dispatch_from_an_array() -> string {
  let held: ValNamed[] = [];
  for (let i = 0; i < 6; i += 1) { held.push(val_pick(i % 3)); };
  let out = "";
  for (let x in held) { out = out + x.named() + ","; };
  out
}

// What is written of a value of unknown type that is a scalar, and what it
// equals.
function val_unknown_json() -> string {
  let i = val_hide(val_count(7), 2);
  let f = val_hide(halves(3), 2);
  let b = val_hide(val_count(1) == 1, 2);
  let n = val_hide(null, 2);
  baml.json.stringify([i, f, b, n])
}
function val_unknown_equal() -> string {
  let i = val_hide(val_count(7), 2);
  let f = val_hide(halves(3), 2);
  let b = val_hide(val_count(1) == 1, 2);
  let seven: unknown = 7;
  let half: unknown = 1.5;
  let yes: unknown = true;
  `${i == seven} ${f == half} ${b == yes} ${i == f} ${b == i} ${i == val_hide(8, 1)}`
}
function val_unknown_text() -> string {
  let held: unknown[] = [val_count(7), halves(3), val_count(1) == 1, rep("t", 2)];
  let out = "";
  for (let x in held) {
    out = out + match (x) {
      let n: int => `int ${n}`,
      let f: float => `float ${f}`,
      let b: bool => `bool ${b}`,
      let s: string => `string ${s}`,
      _ => "other",
    } + ",";
  };
  out
}
function val_unknown_float_keys() -> string {
  let m: map<float, int | string> = {};
  for (let i = 0; i < 4; i += 1) { m[halves(i)] = val_either(i); };
  match (m[1.5]) {
    let n: int => `int ${n}`,
    let s: string => `string ${s} ${m.length()}`,
  }
}

// A value whose type is only known when the program runs is kept current
// across a call that collects, whether it is a reference or not.
function val_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += rep("x", i % 7 + 20).length(); };
  total
}
function val_hold(text: int | string, number: int | string, maybe: string?, part: unknown) -> string {
  let churned = val_churn(200);
  let first = match (text) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  let second = match (number) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  let third = match (part) {
    let s: string => s,
    _ => "other",
  };
  `${first} ${second} ${(maybe ?? "none").length()} ${third} ${churned}`
}
function val_held_across_a_collection() -> string {
  val_hold(val_either(21), val_either(20), rep("m", 30), rep("0123456789", 5).slice(10, 40))
}

// Arguments past the registers are the only references to what they refer
// to, and the callee collects before it reads them.
function val_many(a: int, b: int | string, c: string?, d: int?, e: int | string, f: string, g: ValThing?, h: unknown) -> string {
  let churned = val_churn(100);
  let second = match (b) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  let fifth = match (e) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  let last = match (h) {
    let s: string => s,
    _ => "other",
  };
  `${a} ${second} ${(c ?? "none").length()} ${d ?? -1} ${fifth} ${f.length()} ${if g == null { -1 } else { churned }} ${last}`
}
function val_arguments_on_the_stack() -> string {
  val_many(val_count(1), val_either(31), rep("c", 33), val_maybe(8), val_either(35), rep("f", 36), ValThing { size: 37 }, rep("h", 38))
    + "|"
    + val_many(val_count(2), val_either(30), null, val_maybe(9), val_either(34), rep("f", 3), null, rep("0123456789", 4).slice(5, 35))
}

// A thrown value is a tag and a payload: an `int`, a string and an instance
// are each caught by type, and thrown again as they are.
function val_throw(depth: int, kind: int) -> int throws int | float | string | ValError | ValColor {
  if depth > 0 { return val_throw(depth - 1, kind) + 1; }
  if kind == 1 { throw val_count(5) }
  if kind == 2 { throw rep("thrown", 2) }
  if kind == 3 { throw ValError { code: 7 } }
  if kind == 4 { throw halves(5) }
  if kind == 5 { throw ValColor.Green }
  0
}
function val_throw_by_type() -> string {
  let out = "";
  for (let kind = 0; kind < 6; kind += 1) {
    let text = { `returned ${val_throw(4, kind)}` } catch (e) {
      int => `int ${e + 1}`,
      float => `float ${e}`,
      string => `string ${e.length()}`,
      ValError => `error ${e.code}`,
      ValColor => `color ${e == ValColor.Green}`,
    };
    out = out + text + ",";
  };
  out
}
// An arm that takes only some of them throws the others on.
function val_throw_on(kind: int) -> int throws int | float | string | ValError | ValColor {
  val_throw(3, kind) catch (e) {
    float => -1,
  }
}
function val_rethrown() -> string {
  let out = "";
  for (let kind = 1; kind < 6; kind += 1) {
    let text = { `returned ${val_throw_on(kind)}` } catch (e) {
      int => `int ${e + 1}`,
      string => `string ${e.length()}`,
      ValError => `error ${e.code}`,
      _ => "other",
    };
    out = out + text + ",";
  };
  out
}
function val_uncaught_int() -> int { val_throw(2, 1) }
function val_uncaught_float() -> int { val_throw(2, 4) }
