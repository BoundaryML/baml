//! Every case runs on the interpreter and as a native executable, and the two
//! must end the same way: the same printed result, or the same uncaught
//! panic. The native executable runs every way of `common::WAYS`, which must
//! change nothing.
//!
//! Operands come out of loops (`count`, `half`, `pow2`) so that the
//! optimizer cannot fold a case into a constant and the generated arithmetic
//! is what runs.

use baml_db::testing::{assert_no_diagnostic_errors, setup_test_db};

mod common;
use common::{Ending, WAYS, build, engine, interpret, listed, run};

const SOURCE: &str = r#"
function count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function half(n: int) -> float {
  let x = 0.0;
  for (let i = 0; i < n; i += 1) { x += 0.5; };
  x
}
function pow2(n: int) -> int {
  let x = 1;
  for (let i = 0; i < n; i += 1) { x = x * 2; };
  x
}
function int_max() -> int { let p = pow2(62); (p - 1) + p }
function int_min() -> int { -int_max() - 1 }
function fib(n: int) -> int { if n <= 1 { n } else { fib(n - 1) + fib(n - 2) } }
function pick(x: int) -> int {
  match (x) { 0 => 10, 1 => 20, 7 => 30, _ => 40 }
}
function mix(a: int, b: float, flag: bool) -> float { if flag { a + b } else { b } }

function sum_of_squares() -> int {
  let s = 0;
  for (let i = 0; i < 1000; i += 1) { s += i * i; };
  s
}
function recursion() -> int { fib(20) }
function division_truncates() -> int { -count(7) / count(2) }
function division_by_minus_one() -> int { count(7) / -count(1) }
function remainder_takes_dividend_sign() -> int { -count(7) % count(3) }
function bit_operators() -> int { (count(12) & count(10)) | (count(1) ^ count(3)) }
function product() -> int { pow2(31) * pow2(30) }
function shift_left() -> int { count(3) << count(4) }
function shift_left_past_width() -> int { count(1) << count(64) }
function shift_right_past_width() -> int { -count(8) >> count(70) }
function shift_right() -> int { -count(9) >> count(1) }
function match_on_int() -> int {
  pick(count(0)) + pick(count(1)) * 100 + pick(count(7)) * 10000 + pick(count(9)) * 1000000
}
function mixed_arguments() -> float { mix(count(3), half(1), count(1) == 1) }
function divide_by_zero() -> int { count(5) / count(0) }
function remainder_by_zero() -> int { count(5) % count(0) }
function negative_shift() -> int { count(1) << -count(1) }

function largest_int() -> int { int_max() }
function int_extremes() -> int { int_max() + int_min() }
function shift_into_sign() -> int { count(1) << count(63) }
function add_wraps() -> int { int_max() + count(1) }
function increment_wraps() -> int { int_max() + 1 }
function subtract_wraps() -> int { int_min() - count(1) }
function multiply_wraps() -> int { pow2(32) * pow2(31) }
function multiply_wraps_to_zero() -> int { pow2(40) * pow2(40) }
function negate_wraps() -> int { -int_min() }
function smallest_divided_by_minus_one() -> int { int_min() / -count(1) }
function smallest_remainder_by_minus_one() -> int { int_min() % -count(1) }

function int_times_float() -> float { count(3) * 1.5 + 2 }
function float_shortest_digits() -> float { half(1) / 5.0 + half(1) / 2.5 }
function float_without_exponent() -> float { half(2) * 10000000000000000000000.0 }
function float_small() -> float { half(2) / 3.0 * 0.0000001 }
function float_divide_by_zero() -> float { -half(2) / half(0) }
function float_nan() -> float { half(0) / half(0) }
function float_remainder() -> float { -half(15) % half(4) }
function remainder_by_power_of_two_is_zero() -> int {
  let hits = 0;
  for (let x = -count(9); x < 10; x += 1) {
    if x % 2 == 0 { hits += 1; }
    if x % 4 != 0 { hits += 100; }
    if 0 == x % 8 { hits += 10000; }
    if x % 1 == 0 { hits += 1000000; }
    if x % -2 == 0 { hits += 100000000; }
  }
  if int_min() % 16 == 0 { hits += 10000000000; }
  if int_max() % 16 != 0 { hits += 20000000000; }
  hits
}
function multiply_by_three_five_nine() -> int {
  let x = -count(7);
  (3 * x) + (x * 5) * 100 + (9 * x) * 10000 + int_max() * 3 + int_min() * 9 + count(11) * 5
}
function nan_orders_above_numbers() -> bool {
  let nan = half(0) / half(0);
  let one = half(2);
  nan == nan && nan > one && one < nan && nan >= nan && nan <= nan
    && !(nan < one) && !(one > nan) && nan != one && one <= nan && !(one >= nan)
}
function negative_zero_equals_zero() -> bool { -half(0) == half(0) }

function or_skips_right_side() -> bool { count(3) > count(2) || count(1) / count(0) == 1 }
function and_skips_right_side() -> bool { count(1) > count(2) && count(1) / count(0) == 1 }
function not_of_bool() -> bool { !(count(1) == count(1)) }
function returns_null() -> null { let ignored = count(3); null }

class Point {
  x: int
  y: int
}
class Line {
  from: Point
  to: Point
  label: string
}
enum Color { Red Green Blue }
class Pixel {
  color: Color
  weight: int
  next: Color?
}
function score(c: Color) -> int {
  match (c) {
    Color.Red => 1,
    Color.Green => 2,
    Color.Blue => 3,
  }
}
function colour(n: int) -> Color {
  if count(n) % 3 == 0 { Color.Red } else if n % 3 == 1 { Color.Green } else { Color.Blue }
}
function enum_match() -> int {
  let total = 0;
  for (let i = 0; i < 10; i += 1) {
    let c = colour(i);
    if c == Color.Green { total += 100; };
    if c != Color.Red { total += 1000; };
    total += score(c);
  };
  total
}
function enum_fields() -> int {
  let p = Pixel { color: colour(2), weight: 2, next: null };
  let q = Pixel { color: Color.Red, weight: 5, next: colour(1) };
  p.color = q.color;
  let seen = 0;
  if p.next == null { seen += 10; };
  if q.next != null { seen += 100; };
  score(p.color) * p.weight + score(q.next ?? Color.Blue) * q.weight + seen
}
function enum_array() -> int {
  let colors: Color[] = [];
  for (let i = 0; i < 40; i += 1) { colors.push(colour(i)); };
  let total = 0;
  for (let c in colors) { total += score(c); };
  total
}
class Node {
  value: int
  next: Node?
}
class Tree {
  left: Tree?
  right: Tree?
}
class Mixed {
  ratio: float
  on: bool
  name: string
  items: int[]
}

function instance_fields() -> int {
  let p = Point { x: count(3), y: count(4) };
  p.x * 10 + p.y
}
function field_writes() -> int {
  let p = Point { x: count(1), y: count(2) };
  p.x = count(5);
  p.y = p.x + p.y;
  p.x * 10 + p.y
}
function nested_fields() -> int {
  let l = Line { from: Point { x: count(1), y: 2 }, to: Point { x: 3, y: count(4) }, label: "ab" };
  l.to.x = l.from.y + 10;
  l.from.x + l.to.x * 10 + l.to.y * 100 + l.label.length() * 1000
}
function fields_of_every_kind() -> float {
  let m = Mixed { ratio: half(3), on: count(1) == 1, name: "n", items: [count(4)] };
  m.items.push(6);
  if m.on && m.name == "n" { m.ratio + m.items[0] + m.items[1] } else { 0.0 }
}
function build_list(n: int) -> Node? {
  let head: Node? = null;
  for (let i = 0; i < n; i += 1) { head = Node { value: i, next: head }; };
  head
}
function sum_list(node: Node?) -> int {
  let at = node;
  if (at == null) { return 0; }
  at.value + sum_list(at.next)
}
function linked_list() -> int { sum_list(build_list(count(100))) }
function make_tree(depth: int) -> Tree {
  if (depth <= 0) { return Tree { left: null, right: null }; }
  Tree { left: make_tree(depth - 1), right: make_tree(depth - 1) }
}
function count_tree(node: Tree) -> int {
  let l = node.left;
  let r = node.right;
  if (l == null || r == null) { return 1; }
  1 + count_tree(l) + count_tree(r)
}
function small_trees() -> int {
  let total = 0;
  for (let i = 0; i < 20; i += 1) { total += count_tree(make_tree(4)); };
  total
}
function garbage_under_a_live_tree() -> int {
  let keep = make_tree(14);
  let total = 0;
  for (let i = 0; i < 40; i += 1) { total += count_tree(make_tree(13)); };
  total + count_tree(keep)
}
function growing_live_heap() -> int {
  let all: Point[] = [];
  for (let i = 0; i < 300000; i += 1) { all.push(Point { x: i, y: 1 }); };
  let s = 0;
  for (let p in all) { s += p.y; };
  s + all[299999].x
}

class Reading {
  value: int?
  scale: float?
  ok: bool?
}
function maybe(n: int) -> int? { if n > 2 { n } else { null } }
function optional_ints() -> int {
  let a = maybe(count(5));
  let b = maybe(count(1));
  (a ?? 100) + (b ?? 1000)
}
function optional_null_tests() -> bool {
  let a = maybe(count(5));
  let b = maybe(count(1));
  a != null && b == null
}
function optional_fields() -> float {
  let r = Reading { value: maybe(count(7)), scale: null, ok: count(1) == 1 };
  r.scale = half(3);
  let v = r.value ?? 0;
  let s = r.scale ?? 0.0;
  let ok = r.ok ?? false;
  if ok { v + s } else { 0.0 }
}
function optional_narrowing() -> int {
  let a = maybe(count(9));
  if (a == null) { return 0; }
  a + 1
}
function coalesce_reference() -> int {
  let n = build_list(count(0));
  let m = n ?? Node { value: 42, next: null };
  m.value
}

function array_literal() -> int {
  let a = [count(1), 2, count(3)];
  a[0] + a[1] * 10 + a[2] * 100
}
function array_writes() -> int {
  let a = [1, 2, 3];
  a[1] = count(7);
  a[2] = a[1] + a[0];
  a[0] + a[1] * 10 + a[2] * 100
}
function array_push_and_length() -> int {
  let a: int[] = [];
  for (let i = 0; i < 100; i += 1) { a.push(i * i); };
  a.length() * 1000000 + a[99] + a[50]
}
function array_for_in() -> int {
  let a: int[] = [];
  for (let i = 0; i < 10; i += 1) { a.push(i); };
  let s = 0;
  for (let x in a) { s += x; };
  s
}
function for_in_sees_appends() -> int {
  let a = [1, 2, 3];
  let n = 0;
  for (let x in a) {
    if a.length() < 6 { a.push(x); };
    n += x;
  };
  n
}
function array_of_instances() -> int {
  let ps: Point[] = [];
  for (let i = 0; i < 10; i += 1) { ps.push(Point { x: i, y: i * 2 }); };
  let s = 0;
  for (let p in ps) { s += p.x + p.y; };
  ps[3].x = 100;
  s + ps[3].x
}
function array_of_strings() -> int {
  let ws = ["a", "bc", "def"];
  let tail = "gh";
  for (let i = 0; i < 1; i += 1) { tail = tail + "ij"; };
  ws.push(tail);
  let n = 0;
  for (let w in ws) { n = n * 10 + w.length(); };
  n
}
function nested_arrays() -> int {
  let g: int[][] = [];
  for (let i = 0; i < 3; i += 1) {
    let row: int[] = [];
    for (let j = 0; j < 3; j += 1) { row.push(i * 3 + j); };
    g.push(row);
  };
  g[2][1] + g[1][2] * 10
}
function array_of_floats() -> float {
  let a = [half(1), half(3)];
  a.push(half(4));
  a[0] + a[1] + a[2]
}
function array_of_bools() -> bool {
  let a = [count(1) == 1, count(1) == 2];
  a.push(true);
  a[0] && !a[1] && a[2]
}
function index_past_end() -> int { let a = [1, 2, 3]; a[count(5)] }
function index_from_end() -> int { let a = [1, 2, 3]; a[-count(1)] * 10 + a[-count(3)] }
function index_before_start() -> int { let a = [1, 2, 3]; a[-count(4)] }
function index_from_end_written() -> int {
  let a = [1, 2, 3];
  a[-count(1)] = 7;
  a[2]
}
function counted_neighbours() -> int {
  let a = [5, 1, 4, 2, 3];
  for (let i = 0; i < a.length(); i += 1) {
    for (let j = 0; j < a.length() - i - 1; j += 1) {
      if a[j] > a[j + 1] {
        let tmp = a[j];
        a[j] = a[j + 1];
        a[j + 1] = tmp;
      };
    };
  };
  a[0] * 10000 + a[1] * 1000 + a[2] * 100 + a[3] * 10 + a[4]
}
function counted_one_too_far() -> int {
  let a = [1, 2, 3];
  let total = 0;
  for (let i = 0; i < a.length(); i += 1) { total += a[i + 1]; };
  total
}
function counted_while_growing() -> int {
  let a = [1, 2, 3];
  let total = 0;
  for (let i = 0; i < a.length(); i += 2) {
    total += a[i];
    if a.length() < 6 { a.push(10); };
  };
  total
}
function index_of_empty() -> int { let a: int[] = []; a[count(0)] }

function string_concat_loop() -> int {
  let s = "";
  for (let i = 0; i < 50; i += 1) { s = s + "xy"; };
  s.length()
}
function string_result() -> string {
  let s = "q";
  for (let i = 0; i < 2; i += 1) { s = s + "\"z"; };
  s
}
function many(depth: int, a: Node, b: int, c: Node, d: int, e: Node, f: int, g: Node, h: int, i: Node, j: int, k: Node, l: Node) -> int {
  if depth > 0 {
    return many(depth - 1, a, b, c, d, e, f, g, h, i, j, k, l);
  };
  let fresh = Node { value: b + d + f + h + j, next: l };
  a.value + c.value + e.value + g.value + i.value + k.value + fresh.value + l.value
}
function arguments_on_the_stack() -> int {
  let total = 0;
  for (let n = 0; n < 20; n += 1) {
    let x = Node { value: n, next: null };
    total += many(3, x, 1, Node { value: 2, next: null }, 3, x, 4, Node { value: 5, next: null }, 6, x, 7, x, Node { value: 8, next: x });
  };
  total
}
function string_length_counts_characters() -> int {
  let s = "é";
  for (let i = 0; i < 3; i += 1) { s = s + "日本"; };
  s.length()
}
function string_equality() -> bool {
  let a = "ab";
  let b = "a";
  for (let i = 0; i < 1; i += 1) { b = b + "b"; };
  a == b && !(a != b) && a != "ac"
}
function string_ordering() -> bool {
  let a = "abc";
  let b = "ab";
  for (let i = 0; i < 1; i += 1) { b = b + "d"; };
  a < b && b > a && a <= a && !(b <= a) && "ab" < a && a >= "abc"
}
function string_of_three_parts() -> string {
  let n = "x";
  for (let i = 0; i < 1; i += 1) { n = n + "y"; };
  n + "-" + n + "!"
}
function int_to_string() -> string { count(42).to_string() + "/" + (-count(7)).to_string() }
function template_holes() -> string {
  let n = count(3);
  let ok = n == 3;
  let name = "n";
  `${name}=${n} ok=${ok} half=${half(1)} inf=${half(1) / half(0)}`
}
function strings_survive_collection() -> int {
  let words: string[] = [];
  for (let i = 0; i < 200; i += 1) {
    let w = "w";
    for (let j = 0; j < i % 5; j += 1) { w = w + "o"; };
    words.push(w);
  };
  let n = 0;
  for (let w in words) { n += w.length(); };
  n
}
function apply(f: (int) -> int throws never, x: int) -> int { f(x) }
function make_adder(n: int) -> (int) -> int throws never { (x: int) -> int { x + n } }
function closure_captures_parameter() -> int {
  let add = make_adder(count(5));
  apply(add, 1) + add(10)
}
function double(x: int) -> int { x * 2 }
function pick_function(flag: bool) -> (int) -> int throws never {
  if flag { double } else { make_adder(100) }
}
function function_as_value() -> int {
  let f = pick_function(count(1) == 1);
  let g = pick_function(count(1) == 0);
  f(4) * 1000 + g(4)
}
function make_counter() -> () -> int throws never {
  let n = 0;
  () -> int { n += 1; n }
}
function closure_writes_its_capture() -> int {
  let next = make_counter();
  let other = make_counter();
  next();
  next();
  other();
  next() * 10 + other()
}
function closures_share_a_cell() -> int {
  let total = 0;
  let fs: ((int) -> int throws never)[] = [];
  fs.push((x: int) -> int { total += x; total });
  fs.push((x: int) -> int { total * x });
  let a = fs[count(0)](5);
  let b = fs[count(1)](3);
  total += 1;
  a * 10000 + b * 100 + fs[1](2)
}
function closure_per_iteration() -> int {
  let fs: (() -> int throws never)[] = [];
  for (let i = 0; i < 4; i += 1) { fs.push(() -> int { i * 10 }); }
  let s = 0;
  for (let f in fs) { s = s * 100 + f(); }
  s
}
function closure_captures_references() -> string {
  let name = "a" + count(2).to_string();
  let parts: string[] = [];
  let fs = [(s: string) -> int { parts.push(name + s); parts.length() }];
  fs[count(0)]("x");
  fs[0]("y");
  parts[0] + parts[1]
}
class Tally {
  total: int
  function add(self, x: int) -> int { self.total += x; self.total }
}
function bound_method() -> int {
  let t = Tally { total: count(1) };
  let fs = [t.add];
  fs[count(0)](5) * 100 + t.total
}
function nested_closures() -> int {
  let base = count(3);
  let fs = [(a: int) -> (int) -> int throws never { (b: int) -> int { base += 1; a * b + base } }];
  let f = fs[count(0)](2);
  f(5) * 100 + f(5)
}
function closure_over_optional() -> int {
  let last: int? = null;
  let fs = [(x: int) -> int { let prev = last ?? -1; last = x; prev }];
  fs[count(0)](7) * 100 + fs[0](9) + 1
}
function closure_of_floats() -> float {
  let scale = half(3);
  let fs = [(x: float, n: int) -> float { x * scale + n }];
  fs[count(0)](2.0, 1)
}
class Dog { name: string }
class Cat { lives: int }
enum Shade { Dark Mid Light }
function union_make(i: int) -> int | string | Dog | Cat | Shade | float | bool | null {
  match (i % 8) {
    0 => i,
    1 => "s",
    2 => Dog { name: "rex" },
    3 => Cat { lives: 9 },
    4 => Shade.Mid,
    5 => 1.5,
    6 => true,
    _ => null
  }
}
function union_score(v: int | string | Dog | Cat | Shade | float | bool | null) -> int {
  match (v) {
    let n: int => n,
    let s: string => s.length(),
    let d: Dog => d.name.length() + 100,
    let c: Cat => c.lives,
    let k: Shade => match (k) { Shade.Dark => 6, Shade.Mid => 7, Shade.Light => 8 },
    let f: float => if (f > 1.0) { 2 } else { 0 },
    let b: bool => if (b) { 3 } else { 0 },
    null => -1
  }
}
function union_match() -> int {
  let s = 0;
  for (let i = 0; i < count(16); i += 1) { s += union_score(union_make(i)); }
  s
}
function union_step(v: int | string, depth: int) -> int {
  if (depth > 0) { return union_step(v, depth - 1); }
  if (v is int) { v + 1 } else { v.length() }
}
function union_narrows_by_test() -> int {
  let xs: (int | string)[] = [count(5), "abc"];
  let s = 0;
  for (let x in xs) { s = s * 10 + union_step(x, 1); }
  s
}
function union_exits(v: int | string | null, depth: int) -> int {
  if (depth > 0) { return union_exits(v, depth - 1); }
  if (v == null) { return -1; }
  if (v is string) { return v.length(); }
  v * 2
}
function union_narrows_by_early_return() -> int {
  union_exits(count(4), 1) * 100 + union_exits("abcde", 1) * 10 + union_exits(null, 1)
}
function union_both(v: int | string, depth: int) -> bool {
  if (depth > 0) { return union_both(v, depth - 1); }
  v is int && v > 3
}
function union_narrows_in_and() -> bool {
  union_both(count(5), 1) && !union_both(count(2), 1) && !union_both("x", 1)
}
function unknown_kind(v: unknown, depth: int) -> int {
  if (depth > 0) { return unknown_kind(v, depth - 1); }
  if (v is string) { return 1 + v.length(); }
  if (v is Dog) { return 20; }
  if (v is int) { return 300 + v; }
  if (v == null) { return 4000; }
  50000
}
function unknown_values() -> int {
  unknown_kind("ab", 1) + unknown_kind(Dog { name: "a" }, 1) + unknown_kind(count(7), 1)
    + unknown_kind(null, 1) + unknown_kind(half(3), 1)
}
class Holder { value: int | string | null }
function union_in_a_field() -> int {
  let hs = [Holder { value: count(3) }, Holder { value: "four" }, Holder { value: null }];
  let s = 0;
  for (let h in hs) {
    let v = h.value;
    s = s * 10 + union_exits(v, 0);
  }
  hs[0].value = "seven77";
  s * 10 + union_exits(hs[0].value, 0)
}
function union_of_scalars() -> float {
  let xs: (float | bool | int)[] = [half(3), true, count(2), false];
  let s = 0.0;
  for (let x in xs) {
    match (x) {
      let f: float => { s += f; },
      let b: bool => { if (b) { s += 10.0; } },
      let n: int => { s += n * 100; }
    }
  }
  s
}
function shade_of(n: int) -> Shade { if (n > 1) { Shade.Light } else { Shade.Dark } }
function enum_result() -> Shade { shade_of(count(2)) }
function generic_apply<T>(f: (T) -> T throws never, x: T) -> T { f(x) }
function inlined_generic_call() -> int {
  let double = (x: int) -> int { x * 2 };
  let s = 0;
  for (let i = 0; i < count(10); i += 1) { s += generic_apply(double, i); }
  s
}
class Crate<T> {
  item T
  seen int
}
class Spot { x int }
enum Tone { Low High }
function pick_one<T>(a: T, b: T, n: int) -> T {
  if (n <= 0) { a } else { pick_one(b, a, n - 1) }
}
function crate_of<T>(x: T, n: int) -> Crate<T> {
  if (n <= 0) { Crate { item: x, seen: 0 } } else {
    let c = crate_of(x, n - 1);
    c.seen += 1;
    c
  }
}
function is_a<T>(u: unknown, n: int) -> bool {
  if (n <= 0) { u is T } else { is_a<T>(u, n - 1) }
}
function fill<T>(xs: T[], x: T, n: int) -> T[] {
  if (n > 0) { xs.push(x); fill(xs, x, n - 1) } else { xs }
}
function apply_n<T, U>(x: T, f: (T) -> U throws never, n: int) -> U {
  if (n <= 0) { f(x) } else { apply_n(x, f, n - 1) }
}
function generic_per_representation() -> string {
  let i = pick_one(1, 2, count(3));
  let f = pick_one(1.5, 2.5, count(2));
  let b = pick_one(true, false, count(1));
  let s = pick_one("left", "right", count(1));
  let p = pick_one(Spot { x: 7 }, Spot { x: 9 }, count(4));
  let t = pick_one<Tone>(Tone.Low, Tone.High, count(1));
  let o: int? = pick_one<int?>(null, 5, count(1));
  let m = o ?? 0;
  let n = if (t == Tone.High) { m + 10 } else { m };
  `${i} ${f} ${b} ${s} ${p.x} ${n}`
}
function generic_class_instances() -> int {
  let a = crate_of(40, count(3));
  let b = crate_of("word", count(2));
  let c = crate_of(Spot { x: 5 }, count(1));
  let d = crate_of(2.5, count(4));
  let whole = if (d.item > 2.0) { 1000 } else { 0 };
  a.item + a.seen + b.item.length() * 10 + b.seen + c.item.x * 100 + c.seen + whole + d.seen
}
function generic_type_test() -> int {
  let total = 0;
  if (is_a<int>(4, count(2))) { total += 1; }
  if (is_a<string>("x", count(2))) { total += 10; }
  if (is_a<string>(4, count(1))) { total += 100; }
  if (is_a<Spot>(Spot { x: 1 }, count(3))) { total += 1000; }
  if (is_a<Spot>("spot", count(3))) { total += 10000; }
  if (is_a<float>(1, count(1))) { total += 100000; }
  if (is_a<Tone>(Tone.High, count(1))) { total += 1000000; }
  total
}
function generic_array_fill() -> int {
  let ints: int[] = [];
  let strings: string[] = ["a"];
  let a = fill(ints, 5, count(3));
  let b = fill(strings, "bc", count(4));
  let total = 0;
  for (let x in a) { total += x; }
  for (let s in b) { total += s.length(); }
  total * 10 + b.length()
}
function generic_with_function_value() -> float {
  let offset = count(2);
  let r = apply_n(3, (v: int) -> float { v + 0.5 + offset }, count(2));
  let s = apply_n("four", (v: string) -> int { v.length() }, count(3));
  r + s
}
interface Shape {
  function area(self) -> int throws never
  function scaled(self, by: int) -> int throws never
  function twice(self) -> int throws never { self.area() * 2 }
}
interface Named {
  function label(self) -> string throws never
}
class Square {
  side int
  implements Shape {
    function area(self) -> int throws never { self.side * self.side }
    function scaled(self, by: int) -> int throws never { self.side * by }
  }
  implements Named {
    function label(self) -> string throws never { "square" }
  }
}
class Strip {
  width int
  height int
  implements Shape {
    function area(self) -> int throws never { self.width * self.height }
    function scaled(self, by: int) -> int throws never { self.width * by + self.height }
    function twice(self) -> int throws never { 7 }
  }
}
class Price {
  cents int
  implements baml.ToString {
    function to_string(self) -> string throws never {
      "$" + (self.cents / 100).to_string() + "." + (self.cents % 100).to_string()
    }
  }
}
enum Level { Quiet Loud }
function shape_of(n: int) -> Shape {
  if (n % 2 == 0) { Square { side: n } } else { Strip { width: n, height: 3 } }
}
function interface_static_call() -> int {
  let s = Square { side: count(4) };
  let t = Strip { width: count(2), height: count(5) };
  s.area() + t.area() * 100 + s.twice() * 10000 + t.twice() * 1000000
}
function interface_dispatch() -> int {
  let total = 0;
  for (let i = 0; i < count(10); i += 1) {
    let shape = shape_of(i);
    total += shape.area() + shape.scaled(2) * 1000 + shape.twice() * 1000000;
  }
  total
}
function interface_values_in_array() -> int {
  let shapes: Shape[] = [];
  for (let i = 1; i <= count(6); i += 1) { shapes.push(shape_of(i)); }
  let total = 0;
  for (let shape in shapes) { total = total * 3 + shape.area(); }
  total
}
function describe_it(n: Named) -> string { n.label() }
function interface_two_tables() -> string {
  let s = Square { side: 3 };
  let shape: Shape = s;
  describe_it(s) + shape.area().to_string()
}
function to_string_of_values() -> string {
  let level = if (count(1) > 0) { Level.Loud } else { Level.Quiet };
  let p = Price { cents: count(1234) };
  level.to_string() + " " + p.to_string() + " " + count(5).to_string() + ` ${p} ${level} ${count(2) > 1} ${1.5 * count(3)}`
}
function show(x: baml.ToString) -> string { x.to_string() }
function to_string_dispatch() -> string {
  let level = if (count(1) > 0) { Level.Loud } else { Level.Quiet };
  show(Price { cents: count(250) }) + "|" + show(count(7)) + "|" + show("text") + "|" + show(level) + "|" + show(2.5 * count(1)) + "|" + show(count(1) == 1)
}
function to_string_of_mixed() -> string {
  let level = if (count(1) > 0) { Level.Loud } else { Level.Quiet };
  let xs: baml.ToString[] = [Price { cents: count(250) }, count(7), "text", level, 2.5 * count(1), count(1) == 1];
  let out = "";
  for (let x in xs) { out = out + x.to_string() + "|"; }
  out
}
class Dot {
  implements Named {
    function label(self) -> string throws never { "dot" }
  }
}
function interface_columns() -> string {
  let shapes: Shape[] = [Square { side: count(2) }, Strip { width: 1, height: count(9) }];
  let named: Named[] = [Dot {}, Square { side: 1 }];
  let out = "";
  for (let n in named) { out = out + n.label() + ":"; }
  for (let s in shapes) { out = out + s.twice().to_string() + "," + s.scaled(count(3)).to_string() + ";"; }
  out
}
interface Kind {
  function rank(self) -> int throws never
}
class Ace {
  implements Kind {
    function rank(self) -> int throws never { 14 }
  }
}
class Pip {
  n int
  implements Kind {
    function rank(self) -> int throws never { self.n }
  }
}
class Joker { wild bool }
function classify(u: unknown) -> int {
  if (u is Kind) { 1 } else if (u is int | string) { 2 } else if (u is Joker | null) { 3 } else { 4 }
}
function type_test_sets() -> int {
  let xs: unknown[] = [Ace {}, Pip { n: count(3) }, count(5), "s", Joker { wild: true }, null, 2.5, true];
  let total = 0;
  for (let x in xs) { total = total * 10 + classify(x); }
  total
}
type Texty = string | Joker
function sort_out(u: unknown) -> int {
  match (u) {
    let t: Texty => 1
    let n: int | float | null => 2
    let k: Kind => 3
    _ => 4
  }
}
function type_test_in_match() -> int {
  let xs: unknown[] = [Ace {}, count(5), "s", Joker { wild: true }, null, 2.5, true, Pip { n: 2 }];
  let total = 0;
  for (let x in xs) { total = total * 10 + sort_out(x); }
  total
}
function long_part() -> string {
  let whole = "";
  for (let i = 0; i < count(8); i += 1) { whole = whole + "0123456789"; }
  whole.slice(count(3), count(63))
}
function slice_passes_string_tests() -> string {
  let part = long_part();
  let total = 0;
  if (is_a<string>(part, count(2))) { total += 1; }
  if (is_a<int>(part, count(2))) { total += 10; }
  if (is_a<Spot>(part, count(1))) { total += 100; }
  let us: unknown[] = [part, count(1)];
  let hs = [Holder { value: part }, Holder { value: null }];
  let u = us[count(0)];
  `${total}|${unknown_kind(u, 1)}|${union_exits(hs[count(0)].value, 1)}|${classify(u)}|${sort_out(u)}`
}
function slice_to_string_dispatch() -> string {
  let xs: baml.ToString[] = [long_part(), count(7), "text"];
  let out = show(long_part()) + "|";
  for (let x in xs) { out = out + x.to_string() + "|"; }
  out
}
class Marker { id int }
function min_max(xs: int[], n: int) -> (int, int) {
  if (n >= xs.length()) { return (xs[0], xs[0]); }
  let rest = min_max(xs, n + 1);
  let lo = if (xs[n] < rest.0) { xs[n] } else { rest.0 };
  let hi = if (xs[n] > rest.1) { xs[n] } else { rest.1 };
  (lo, hi)
}
function tagged(n: int) -> (string, float, Marker, int?) {
  if (n <= 0) { ("end", 0.5, Marker { id: 0 }, null) } else {
    let t = tagged(n - 1);
    (t.0 + "+", t.1 + 1.0, Marker { id: t.2.id + n }, n)
  }
}
function tuple_results() -> int {
  let xs = [count(5), 3, 9, 1, 7];
  let all = min_max(xs, 0);
  let pair = min_max(xs, 3);
  all.0 * 1000 + all.1 * 100 + pair.0 * 10 + pair.1
}
function tuple_of_mixed() -> string {
  let t = tagged(count(3));
  let last = t.3 ?? -1;
  `${t.0} ${t.1} ${t.2.id} ${last}`
}
function tuples_in_array() -> int {
  let pairs: (int, string)[] = [];
  for (let i = 0; i < count(4); i += 1) { pairs.push((i, "x")); }
  let total = 0;
  for (let p in pairs) { total += p.0 + p.1.length() * 10; }
  total
}
function op_sum<T extends baml.ops.Add<T, Output = T>>(a: T, b: T) -> T { a.add(b) }
function op_same<T extends baml.ops.Equals>(a: T, b: T) -> bool { a.eq(b) }
function operator_methods_on_int() -> int {
  let a = count(7);
  let b = count(3);
  let sum = a.as<baml.ops.Add<int, Output = int>>.add(b);
  let difference = a.as<baml.ops.Subtract<int, Output = int>>.sub(b);
  let product = a.as<baml.ops.Multiply<int, Output = int>>.mul(b);
  let quotient = a.as<baml.ops.Divide<int, Output = int>>.div(b);
  let remainder = a.as<baml.ops.Remainder<int, Output = int>>.rem(b);
  let negated = a.as<baml.ops.Negate<Output = int>>.neg();
  let bits = a.as<baml.ops.BitAnd<int, Output = int>>.bit_and(b) * 100
    + a.as<baml.ops.BitOr<int, Output = int>>.bit_or(b) * 10
    + a.as<baml.ops.BitXor<int, Output = int>>.bit_xor(b);
  let shifts = a.as<baml.ops.ShiftLeft<int, Output = int>>.shl(b) * 10
    + a.as<baml.ops.ShiftRight<int, Output = int>>.shr(count(1));
  sum + difference * 100 + product * 1000 + quotient * 100000 + remainder * 1000000
    + negated * 10000000 + bits * 100000000 + shifts * 100000000000
}
function operator_methods_on_float() -> float {
  let a = half(5);
  let b = half(1);
  let n = count(2);
  a.as<baml.ops.Add<float, Output = float>>.add(b)
    + a.as<baml.ops.Subtract<float, Output = float>>.sub(b) * 10.0
    + a.as<baml.ops.Multiply<int, Output = float>>.mul(n) * 100.0
    + n.as<baml.ops.Divide<float, Output = float>>.div(b) * 1000.0
    + a.as<baml.ops.Remainder<float, Output = float>>.rem(b + b + b) * 10000.0
    + a.as<baml.ops.Negate<Output = float>>.neg() * 100000.0
}
function operator_equals() -> int {
  let seen = 0;
  if count(3).eq(count(3)) { seen += 1; }
  if count(3).neq(count(4)) { seen += 10; }
  if half(1).eq(half(1)) { seen += 100; }
  if (count(1) == 1).eq(count(2) == 2) { seen += 1000; }
  if (half(0) / half(0)).eq(half(0) / half(0)) { seen += 10000; }
  seen
}
function operator_methods_of_a_type_parameter() -> int {
  let seen = op_sum(count(3), count(4));
  if op_sum(half(3), half(4)) == 3.5 { seen += 10; }
  if op_same(count(2), count(2)) { seen += 100; }
  if op_same(half(2), half(3)) { seen += 1000; }
  seen
}
function operator_method_divides_by_zero() -> int {
  count(5).as<baml.ops.Divide<int, Output = int>>.div(count(0))
}
function operator_method_shifts_by_negative() -> int {
  count(5).as<baml.ops.ShiftLeft<int, Output = int>>.shl(-count(1))
}
"#;

/// How a run ended. `baml run` prints a result on stdout and exits zero, or
/// reports an uncaught panic on stderr and exits one.
#[derive(Debug, PartialEq, Eq)]
enum Outcome<'a> {
    Printed(&'a str),
    Panicked(&'a str),
}
use Outcome::{Panicked, Printed};

struct Case {
    function: &'static str,
    /// How native code must end.
    expected: Outcome<'static>,
    /// Whether the interpreter must end the same way. It does not where the
    /// answer depends on `int` being 64 bits wide and wrapping: the
    /// interpreter's `int` is still 63 bits wide and panics on overflow.
    interpreter_agrees: bool,
    /// Whether the case is small enough to run collecting on every
    /// allocation.
    stressed: bool,
}

const fn case(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: true,
        stressed: true,
    }
}

/// A case that allocates enough to collect and to grow the heap on its own.
const fn heavy(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: true,
        stressed: false,
    }
}

/// A case the interpreter is expected to diverge on until its `int` wraps
/// at 64 bits too.
const fn wide(function: &'static str, expected: Outcome<'static>) -> Case {
    Case {
        function,
        expected,
        interpreter_agrees: false,
        stressed: true,
    }
}

const CASES: &[Case] = &[
    case("sum_of_squares", Printed("332833500")),
    case("recursion", Printed("6765")),
    case("division_truncates", Printed("-3")),
    case("division_by_minus_one", Printed("-7")),
    case("remainder_takes_dividend_sign", Printed("-1")),
    case("bit_operators", Printed("10")),
    case("product", Printed("2305843009213693952")),
    case("shift_left", Printed("48")),
    case("shift_left_past_width", Printed("0")),
    case("shift_right_past_width", Printed("-1")),
    case("shift_right", Printed("-5")),
    case("match_on_int", Printed("40302010")),
    case("mixed_arguments", Printed("3.5")),
    case(
        "divide_by_zero",
        Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    case(
        "remainder_by_zero",
        Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    case(
        "negative_shift",
        Panicked("baml.panics.NegativeBitShift {message: \"bit shift count is negative: -1\"}"),
    ),
    wide("largest_int", Printed("9223372036854775807")),
    wide("int_extremes", Printed("-1")),
    wide("shift_into_sign", Printed("-9223372036854775808")),
    wide("add_wraps", Printed("-9223372036854775808")),
    wide("increment_wraps", Printed("-9223372036854775808")),
    wide("subtract_wraps", Printed("9223372036854775807")),
    wide("multiply_wraps", Printed("-9223372036854775808")),
    wide("multiply_wraps_to_zero", Printed("0")),
    wide("negate_wraps", Printed("-9223372036854775808")),
    wide(
        "smallest_divided_by_minus_one",
        Printed("-9223372036854775808"),
    ),
    wide("remainder_by_power_of_two_is_zero", Printed("30919031409")),
    wide("multiply_by_three_five_nine", Printed("-633469")),
    wide("smallest_remainder_by_minus_one", Printed("0")),
    case("int_times_float", Printed("6.5")),
    case("float_shortest_digits", Printed("0.30000000000000004")),
    case(
        "float_without_exponent",
        Printed("10000000000000000000000.0"),
    ),
    case("float_small", Printed("0.00000003333333333333333")),
    case("float_divide_by_zero", Printed("-inf")),
    case("float_nan", Printed("NaN")),
    case("float_remainder", Printed("-1.5")),
    case("nan_orders_above_numbers", Printed("true")),
    case("negative_zero_equals_zero", Printed("true")),
    case("or_skips_right_side", Printed("true")),
    case("and_skips_right_side", Printed("false")),
    case("not_of_bool", Printed("false")),
    case("returns_null", Printed("")),
    case("instance_fields", Printed("34")),
    case("field_writes", Printed("57")),
    case("nested_fields", Printed("2521")),
    case("fields_of_every_kind", Printed("11.5")),
    case("enum_match", Printed("6319")),
    case("enum_fields", Printed("122")),
    case("enum_array", Printed("79")),
    case("linked_list", Printed("4950")),
    case("small_trees", Printed("620")),
    // More arguments than registers: the collector finds a caller's frame
    // from its callee's whatever the arguments take on the stack.
    case("arguments_on_the_stack", Printed("1480")),
    heavy("garbage_under_a_live_tree", Printed("688087")),
    heavy("growing_live_heap", Printed("599999")),
    case("optional_ints", Printed("1005")),
    case("optional_null_tests", Printed("true")),
    case("optional_fields", Printed("8.5")),
    case("optional_narrowing", Printed("10")),
    case("coalesce_reference", Printed("42")),
    case("array_literal", Printed("321")),
    case("array_writes", Printed("871")),
    case("array_push_and_length", Printed("100012301")),
    case("array_for_in", Printed("45")),
    case("for_in_sees_appends", Printed("12")),
    case("array_of_instances", Printed("235")),
    case("array_of_strings", Printed("1234")),
    case("nested_arrays", Printed("57")),
    case("array_of_floats", Printed("4.0")),
    case("array_of_bools", Printed("true")),
    case(
        "index_past_end",
        Panicked("baml.panics.IndexOutOfBounds {index: 5, length: 3}"),
    ),
    case("index_from_end", Printed("31")),
    case(
        "index_before_start",
        Panicked("baml.panics.IndexOutOfBounds {index: -4, length: 3}"),
    ),
    case("index_from_end_written", Printed("7")),
    case("counted_neighbours", Printed("12345")),
    case(
        "counted_one_too_far",
        Panicked("baml.panics.IndexOutOfBounds {index: 3, length: 3}"),
    ),
    case("counted_while_growing", Printed("14")),
    case(
        "index_of_empty",
        Panicked("baml.panics.IndexOutOfBounds {index: 0, length: 0}"),
    ),
    case("string_concat_loop", Printed("100")),
    case("string_result", Printed("\"q\\\"z\\\"z\"")),
    case("string_length_counts_characters", Printed("7")),
    case("string_equality", Printed("true")),
    case("string_ordering", Printed("true")),
    case("string_of_three_parts", Printed("\"xy-xy!\"")),
    case("int_to_string", Printed("\"42/-7\"")),
    case(
        "template_holes",
        Printed("\"n=3 ok=true half=0.5 inf=Infinity\""),
    ),
    case("strings_survive_collection", Printed("600")),
    case("closure_captures_parameter", Printed("21")),
    case("function_as_value", Printed("8104")),
    case("closure_writes_its_capture", Printed("32")),
    case("closures_share_a_cell", Printed("51512")),
    case("closure_per_iteration", Printed("102030")),
    case("closure_captures_references", Printed("\"a2xa2y\"")),
    case("bound_method", Printed("606")),
    case("nested_closures", Printed("1415")),
    case("closure_over_optional", Printed("-92")),
    case("closure_of_floats", Printed("4.0")),
    case("union_match", Printed("256")),
    case("union_narrows_by_test", Printed("63")),
    case("union_narrows_by_early_return", Printed("849")),
    case("union_narrows_in_and", Printed("true")),
    case("unknown_values", Printed("54330")),
    case("union_in_a_field", Printed("6397")),
    case("union_of_scalars", Printed("211.5")),
    case("enum_result", Printed("Light")),
    case("inlined_generic_call", Printed("90")),
    case(
        "generic_per_representation",
        Printed("\"2 1.5 false right 7 15\""),
    ),
    case("generic_class_instances", Printed("1590")),
    case("generic_type_test", Printed("1001011")),
    case("generic_array_fill", Printed("245")),
    case("generic_with_function_value", Printed("9.5")),
    case("interface_static_call", Printed("7321016")),
    case("interface_dispatch", Printed("275105195")),
    case("interface_values_in_array", Printed("1521")),
    case("interface_two_tables", Printed("\"square9\"")),
    case("interface_columns", Printed("\"dot:square:8,6;7,12;\"")),
    case(
        "to_string_of_values",
        Printed("\"Loud $12.34 5 $12.34 Loud true 4.5\""),
    ),
    case(
        "to_string_dispatch",
        Printed("\"$2.50|7|text|Loud|2.5|true\""),
    ),
    case(
        "to_string_of_mixed",
        Printed("\"$2.50|7|text|Loud|2.5|true|\""),
    ),
    case("type_test_sets", Printed("11223344")),
    case("type_test_in_match", Printed("32112243")),
    case("slice_passes_string_tests", Printed("\"1|61|60|2|1\"")),
    case(
        "slice_to_string_dispatch",
        Printed(
            "\"345678901234567890123456789012345678901234567890123456789012|345678901234567890123456789012345678901234567890123456789012|7|text|\"",
        ),
    ),
    case("tuple_results", Printed("1917")),
    case("tuple_of_mixed", Printed("\"end+++ 3.5 6 3\"")),
    case("tuples_in_array", Printed("46")),
    case("operator_methods_on_int", Printed("56337331221410")),
    case("operator_methods_on_float", Printed("-235477.0")),
    case("operator_equals", Printed("11111")),
    case("operator_methods_of_a_type_parameter", Printed("117")),
    case(
        "operator_method_divides_by_zero",
        Panicked("baml.panics.DivisionByZero {dividend: 5}"),
    ),
    case(
        "operator_method_shifts_by_negative",
        Panicked("baml.panics.NegativeBitShift {message: \"bit shift count is negative: -1\"}"),
    ),
];

fn outcome(result: &Ending) -> String {
    match result {
        Ok(text) => format!("{:?}", Printed(text)),
        Err(text) => format!("{:?}", Panicked(text)),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn native_code_ends_as_the_interpreter_does() {
    let db = setup_test_db(SOURCE);
    assert_no_diagnostic_errors(&db);
    let engine = engine(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut failures = Vec::new();
    for case in CASES {
        let expected = format!("{:?}", case.expected);
        let built = build(&db, case.function, directory.path())
            .unwrap_or_else(|error| panic!("`{}` {error}", case.function));
        let native: Vec<(&str, String)> = WAYS
            .iter()
            .filter(|way| case.stressed || !way.variables.contains(&"BAML_GC_STRESS"))
            .map(|way| (way.name, outcome(&run(&built, way))))
            .collect();
        let interpreted = outcome(&interpret(&engine, case.function).await);
        // A case marked as diverging that the interpreter starts agreeing on
        // fails too, so that the mark is removed when it stops being true.
        if native.iter().any(|(_, ending)| *ending != expected)
            || (interpreted == expected) != case.interpreter_agrees
        {
            failures.push(format!(
                "{}:\n  {:<24} {expected}{}\n  {:<24} {interpreted}{}",
                case.function,
                "expected",
                listed(&native),
                "interpreter",
                if case.interpreter_agrees {
                    ""
                } else {
                    " (expected to differ)"
                },
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
