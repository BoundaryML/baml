// Classes, enums and types as objects. Every instance says its class by
// the class object it holds, a value of an enum is its variant's object,
// and a type argument is a type object: dispatch, type tests, `match`,
// handlers and what the runtime writes all read those.

class ObjPoint { x: int y: int }
class ObjLine { length: int }
class ObjError { code: int }
class ObjBox<T> { value: T count: int }
enum ObjColor {
  Red
  Green
  Blue
}
enum ObjSide {
  Left
  Right
}

function obj_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function obj_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += rep("x", i % 7 + 20).length(); };
  total
}
function obj_color(i: int) -> ObjColor {
  if i % 3 == 0 { ObjColor.Red } else if i % 3 == 1 { ObjColor.Green } else { ObjColor.Blue }
}
function obj_side(i: int) -> ObjSide {
  if i % 2 == 0 { ObjSide.Left } else { ObjSide.Right }
}
// What only a static type could tell apart, held where nothing says it.
function obj_hide(value: unknown, n: int) -> unknown {
  let held: unknown[] = [];
  for (let i = 0; i < n; i += 1) { held.push(value); };
  held[n - 1]
}

// An enum's variant is a key of a map: found again, set again, and kept in
// the order it was first set, across collections.
function obj_enum_keys() -> string {
  let m: map<ObjColor, int> = {};
  for (let i = 0; i < 7; i += 1) {
    let c = obj_color(obj_count(8 - i));
    m[c] = (m.get(c) ?? 0) + i;
    obj_churn(40);
  };
  let out = "";
  for (let key in m.keys()) { out = out + `${key}=${m[key]} `; };
  let missing = m.get(obj_color(4)) ?? -1;
  m.delete(ObjColor.Blue);
  `${out}${m.length()} ${missing} ${m.get(ObjColor.Blue) ?? -1} ${m[obj_color(3)]}`
}
function obj_enum_keys_many() -> string {
  let m: map<ObjSide, string> = {};
  let n: map<ObjColor, ObjSide> = {};
  for (let i = 0; i < 40; i += 1) {
    m[obj_side(i)] = (m.get(obj_side(i)) ?? "") + `${i % 10}`;
    n[obj_color(i)] = obj_side(i);
  };
  baml.sys.collect_garbage();
  `${m[ObjSide.Left].length()} ${m[ObjSide.Right]} ${n[ObjColor.Red]} ${n[ObjColor.Green]} ${n[ObjColor.Blue]} ${n.length()}`
}

// A variant in a union, and in an array held as `unknown`: what it is
// written as, what it equals and what `baml.json` writes of it.
function obj_either(i: int) -> ObjColor | ObjSide | int | string {
  if i % 4 == 0 { obj_color(i) } else if i % 4 == 1 { obj_side(i) } else if i % 4 == 2 { i } else { rep("s", i) }
}
function obj_enum_in_a_union() -> string {
  let out = "";
  for (let i = 0; i < 8; i += 1) {
    out = out + match (obj_either(i)) {
      let c: ObjColor => `color ${c}`,
      let s: ObjSide => `side ${s}`,
      let n: int => `int ${n}`,
      let t: string => `string ${t}`,
    } + ",";
  };
  out
}
function obj_enum_held_as_unknown() -> string {
  let held: unknown[] = [];
  for (let i = 0; i < 4; i += 1) { held.push(obj_color(i)); };
  held.push(obj_side(1));
  let out = "";
  for (let x in held) { out = out + `${x} `; };
  let red: unknown = ObjColor.Red;
  let left: unknown = ObjSide.Left;
  `${out}${held[0] == red} ${held[1] == red} ${held[3] == held[0]} ${held[4] == left} ${held[0] == left}`
}
function obj_enum_unknown_json() -> string {
  let held: unknown[] = [obj_color(1), obj_side(1), obj_hide(obj_color(2), 3)];
  baml.json.stringify(held)
}
function obj_optional_enum() -> string {
  let held: ObjColor?[] = [obj_color(1), null, obj_color(2)];
  let out = "";
  for (let x in held) {
    out = out + match (x) {
      null => "none",
      let c: ObjColor => `${c}`,
    } + " ";
  };
  let found = held[obj_count(2)] ?? ObjColor.Red;
  `${out}${found == ObjColor.Blue} ${held[1] == null}`
}

// A variant is the same variant after the collector has run, wherever it
// was held.
class ObjHolder { color: ObjColor side: ObjSide? any: unknown }
function obj_enum_after_a_collection() -> string {
  let holder = ObjHolder { color: obj_color(2), side: obj_side(1), any: obj_color(1) };
  let local = obj_color(2);
  let held: ObjColor[] = [obj_color(0), obj_color(1), obj_color(2)];
  obj_churn(200);
  baml.sys.collect_garbage();
  let green: unknown = ObjColor.Green;
  `${holder.color == local} ${holder.color == held[2]} ${held[0] == ObjColor.Red} ${holder.side == ObjSide.Right} ${holder.any == green} ${local}`
}

// An interface a class, an enum, an `int` and a `string` implement: a call
// on a value of it finds each one's code, and a test says which values
// are of it.
interface ObjNamed {
  function named(self) -> string throws never
}
implements ObjNamed for ObjPoint {
  function named(self) -> string throws never { `point ${self.x + self.y}` }
}
implements ObjNamed for ObjColor {
  function named(self) -> string throws never { `color ${self}` }
}
implements ObjNamed for int {
  function named(self) -> string throws never { `int ${self + 1}` }
}
implements ObjNamed for string {
  function named(self) -> string throws never { `string ${self.length()}` }
}
function obj_pick(i: int) -> ObjNamed {
  if i == 0 { ObjPoint { x: obj_count(3), y: 4 } } else if i == 1 { obj_color(2) } else if i == 2 { obj_count(41) } else { rep("ab", 2) }
}
function obj_dispatch() -> string {
  let held: ObjNamed[] = [];
  for (let i = 0; i < 8; i += 1) { held.push(obj_pick(i % 4)); };
  obj_churn(100);
  let out = "";
  for (let x in held) { out = out + x.named() + ","; };
  out
}
function obj_any(i: int) -> unknown {
  if i == 0 { obj_hide(ObjPoint { x: 1, y: 2 }, 2) }
  else if i == 1 { obj_hide(obj_color(1), 2) }
  else if i == 2 { obj_hide(obj_count(5), 2) }
  else if i == 3 { obj_hide(rep("t", 2), 2) }
  else if i == 4 { obj_hide(ObjLine { length: 3 }, 2) }
  else if i == 5 { obj_hide(obj_side(1), 2) }
  else if i == 6 { obj_hide(halves(3), 2) }
  else if i == 7 { obj_hide(rep("a much longer text", 2).slice(3, 30), 2) }
  else { obj_hide(null, 2) }
}
function obj_is_an_interface() -> string {
  let out = "";
  for (let i = 0; i < 9; i += 1) {
    let x = obj_any(i);
    out = out + if x is ObjNamed { "y" } else { "n" };
  };
  out
}
function obj_is_a_class() -> string {
  let out = "";
  for (let i = 0; i < 9; i += 1) {
    let x = obj_any(i);
    let point = if x is ObjPoint { "p" } else { "-" };
    let line = if x is ObjLine { "l" } else { "-" };
    let color = if x is ObjColor { "c" } else { "-" };
    let side = if x is ObjSide { "s" } else { "-" };
    out = out + point + line + color + side + " ";
  };
  out
}

// A `match` over classes, an enum and scalars finds each arm.
function obj_several(i: int) -> ObjPoint | ObjLine | ObjError | ObjColor | int | null {
  if i == 0 { ObjPoint { x: i, y: 7 } }
  else if i == 1 { ObjLine { length: obj_count(9) } }
  else if i == 2 { ObjError { code: 3 } }
  else if i == 3 { obj_color(i + 1) }
  else if i == 4 { obj_count(i) }
  else { null }
}
function obj_match_over_classes() -> string {
  let out = "";
  for (let i = 0; i < 6; i += 1) {
    out = out + match (obj_several(i)) {
      let p: ObjPoint => `point ${p.y}`,
      let l: ObjLine => `line ${l.length}`,
      let e: ObjError => `error ${e.code}`,
      let c: ObjColor => `color ${c}`,
      let n: int => `int ${n}`,
      null => "null",
    } + ",";
  };
  out
}

// Instances of one class at two type arguments, held where nothing says
// which: each reads its own fields, and the runtime writes each.
function obj_box_int(i: int) -> ObjBox<int> { ObjBox { value: obj_count(i), count: i } }
function obj_box_string(i: int) -> ObjBox<string> { ObjBox { value: rep("b", i), count: i } }
function obj_generic_instances() -> string {
  let ints: ObjBox<int>[] = [];
  let strings: ObjBox<string>[] = [];
  let held: unknown[] = [];
  for (let i = 1; i < 4; i += 1) {
    ints.push(obj_box_int(i));
    strings.push(obj_box_string(i));
    held.push(ints[i - 1]);
    held.push(strings[i - 1]);
  };
  obj_churn(100);
  baml.sys.collect_garbage();
  let out = "";
  for (let b in ints) { out = out + `${b.value + b.count} `; };
  for (let b in strings) { out = out + `${b.value}${b.count} `; };
  `${out}${held.length()} ${held[0] == held[0]} ${held[0] == held[2]}`
}
function obj_thrown_box() -> int throws ObjBox<int> | ObjBox<string> {
  if obj_count(1) == 1 { throw obj_box_string(2) }
  0
}

// A function tests a value against its type argument, which is a class,
// an enum, an `int` or a `string`.
function obj_is<T>(x: unknown) -> bool {
  match (x) {
    let found: T => true,
    _ => false,
  }
}
function obj_tested_against_a_type_argument() -> string {
  let out = "";
  for (let i = 0; i < 9; i += 1) {
    let x = obj_any(i);
    let point = if obj_is<ObjPoint>(x) { "p" } else { "-" };
    let color = if obj_is<ObjColor>(x) { "c" } else { "-" };
    let number = if obj_is<int>(x) { "i" } else { "-" };
    let text = if obj_is<string>(x) { "s" } else { "-" };
    out = out + point + color + number + text + " ";
  };
  out
}
// A type argument that takes more than a tag and a class object to
// decide: a union, a literal, one variant.
function obj_tested_against_a_union() -> string {
  let out = "";
  for (let i = 0; i < 9; i += 1) {
    let x = obj_any(i);
    let either = if obj_is<int | string>(x) { "e" } else { "-" };
    let object = if obj_is<ObjPoint | ObjColor | null>(x) { "o" } else { "-" };
    let literal = if obj_is<"tt">(x) { "l" } else { "-" };
    let five = if obj_is<5 | "bb">(x) { "5" } else { "-" };
    let green = if obj_is<ObjColor.Green>(x) { "g" } else { "-" };
    out = out + either + object + literal + five + green + " ";
  };
  out
}

// What is thrown is caught by its class or its enum, a panic the runtime
// makes among them, in a task and outside one.
function obj_fail(kind: int) -> int throws ObjError | ObjColor | ObjPoint {
  if kind == 1 { throw ObjError { code: obj_count(7) } }
  if kind == 2 { throw obj_color(2) }
  if kind == 3 { throw ObjPoint { x: 1, y: obj_count(2) } }
  if kind == 4 { return 10 / obj_count(0); }
  if kind == 5 { let empty: int[] = []; return empty[obj_count(3)]; }
  kind
}
function obj_caught(kind: int) -> string {
  { `returned ${obj_fail(kind)}` } catch (e) {
    ObjError => `error ${e.code}`,
    ObjColor => `color ${e}`,
    baml.panics.DivisionByZero => "division by zero",
    baml.panics.IndexOutOfBounds => `index ${e.index} of ${e.length}`,
    ObjPoint => `point ${e.y}`,
  }
}
function obj_caught_by_class() -> string {
  let out = "";
  for (let kind = 0; kind < 6; kind += 1) { out = out + obj_caught(kind) + ","; };
  out
}
function obj_caught_in_a_task() -> string {
  let out = "";
  for (let kind = 0; kind < 6; kind += 1) {
    let f = spawn { obj_caught(obj_count(kind)) };
    out = out + (await f) + ",";
  };
  out
}
function obj_caught_from_a_task() -> string {
  let out = "";
  for (let kind = 0; kind < 6; kind += 1) {
    let f = spawn { obj_fail(obj_count(kind)) };
    let text = { `returned ${await f}` } catch (e) {
      ObjError => `error ${e.code}`,
      ObjColor => `color ${e == ObjColor.Blue}`,
      ObjPoint => `point ${e.y}`,
      baml.panics.DivisionByZero => "division by zero",
      _ => "another panic",
    };
    out = out + text + ",";
  };
  out
}
// A handler for one class lets an instance of another pass.
function obj_passes_on(kind: int) -> int throws ObjError | ObjColor | ObjPoint {
  obj_fail(kind) catch (e) {
    ObjPoint => -1,
  }
}
function obj_passed_on() -> string {
  let out = "";
  for (let kind = 1; kind < 4; kind += 1) {
    let text = { `returned ${obj_passes_on(kind)}` } catch (e) {
      ObjError => `error ${e.code}`,
      ObjColor => `color ${e}`,
      _ => "other",
    };
    out = out + text + ",";
  };
  out
}
function obj_uncaught_error() -> int { obj_fail(1) }
function obj_uncaught_variant() -> int { obj_fail(2) }
function obj_uncaught_generic() -> int { obj_thrown_box() }
