// Code with type parameters is compiled once, whatever types it is called
// with: a value of a type parameter is held with its tag, and what the code
// needs to know of the type it reads from the type object it is passed or
// finds through the value's own row.

class GenPoint { x: int y: int }
class GenBox<T> { value: T count: int }
class GenPair<A, B> { first: A second: B }
enum GenColor {
  Red
  Green
  Blue
}

function gen_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function gen_text(n: int) -> string {
  let out = "";
  for (let i = 0; i < n; i += 1) { out = out + "ab"; };
  out
}
// The loop at the start of each function below keeps it from being inlined
// into its caller, where its type parameters would be known.
function gen_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += gen_text(i % 7 + 20).length(); };
  total
}

// A value goes in and comes back as what it was, at every kind of type.
function gen_same<T>(x: T) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  gen_churn(3);
  x
}
function gen_second<A, B>(a: A, b: B) -> B {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let kept = b;
  gen_churn(2);
  kept
}
function gen_at_every_type() -> string {
  let i = gen_same<int>(gen_count(41)) + 1;
  let f = gen_same<float>(1.5) * 2.0;
  let b = gen_same<bool>(gen_count(2) == 2);
  let s = gen_same<string>(gen_text(2)) + "!";
  let p = gen_same<GenPoint>(GenPoint { x: gen_count(3), y: 4 });
  let c = gen_same<GenColor>(GenColor.Green);
  let none: int? = null;
  let some: int? = gen_count(7);
  let n = gen_same<int?>(none) ?? -1;
  let m = gen_same<int?>(some) ?? -1;
  let t = gen_same<string?>(null) ?? "none";
  `${i} ${f} ${b} ${s} ${p.x + p.y} ${c} ${n} ${m} ${t}`
}
function gen_two_parameters() -> string {
  let a = gen_second<int, string>(1, gen_text(1));
  let b = gen_second<string, float>("x", 2.5);
  let c = gen_second<GenPoint, bool>(GenPoint { x: 1, y: 2 }, true);
  let d = gen_second<bool, GenPoint>(false, GenPoint { x: gen_count(5), y: 6 });
  `${a} ${b} ${c} ${d.x * d.y}`
}

// An array of a type parameter's type is made, filled and returned.
function gen_repeat<T>(x: T, n: int) -> T[] {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let out: T[] = [];
  for (let i = 0; i < n; i += 1) {
    out.push(x);
    gen_churn(2);
  };
  out
}
function gen_two<T>(a: T, b: T) -> T[] {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  [a, b, a]
}
function gen_makes_arrays() -> string {
  let ints = gen_repeat<int>(gen_count(7), 3);
  ints[1] = ints[0] + ints[2];
  let floats = gen_repeat<float>(0.5, 2);
  floats[0] = floats[1] + 1.0;
  let texts = gen_two<string>(gen_text(1), "c");
  let points = gen_repeat<GenPoint>(GenPoint { x: 2, y: 3 }, 2);
  let colors = gen_two<GenColor>(GenColor.Red, GenColor.Blue);
  let flags = gen_two<bool>(true, false);
  flags[2] = false;
  `${ints[0]} ${ints[1]} ${ints.length()} ${floats[0]} ${floats[1]} ${texts[0]}${texts[1]}${texts[2]} ${points[1].x + points[0].y} ${colors[1]} ${flags[0]} ${flags[2]}`
}

// An instance of a class with type parameters is made from them.
function gen_boxed<T>(x: T, n: int) -> GenBox<T> {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  gen_churn(2);
  GenBox<T> { value: x, count: n }
}
function gen_paired<A, B>(a: A, b: B) -> GenPair<B, A> {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  GenPair<B, A> { first: b, second: a }
}
function gen_unboxed<T>(box: GenBox<T>) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  gen_churn(2);
  box.value
}
function gen_makes_instances() -> string {
  let a = gen_boxed<int>(gen_count(5), 1);
  a.value = a.value + a.count;
  let b = gen_boxed<string>(gen_text(2), 2);
  let c = gen_boxed<GenPoint>(GenPoint { x: 1, y: 2 }, 3);
  let d = gen_boxed<float>(2.5, 4);
  d.value = d.value * 2.0;
  let e = gen_paired<int, string>(7, "seven");
  let f = gen_paired<bool, GenColor>(true, GenColor.Blue);
  `${a.value} ${b.value}${b.count} ${c.value.y} ${d.value} ${e.first}${e.second} ${f.first}${f.second} ${gen_unboxed<int>(a)} ${gen_unboxed<string>(b)} ${gen_unboxed<GenPoint>(c).x}`
}

// One function with type parameters calls another, with its own and with
// types made of them.
function gen_first<T>(xs: T[]) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  xs[0]
}
function gen_wrap_twice<T>(x: T) -> GenBox<GenBox<T>> {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let inner = gen_boxed<T>(x, 1);
  gen_boxed<GenBox<T>>(inner, 2)
}
function gen_first_of_repeated<T>(x: T, n: int) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let all = gen_repeat<T>(x, n);
  gen_first<T>(all)
}
function gen_calls_another() -> string {
  let a = gen_first_of_repeated<int>(gen_count(9), 2);
  let b = gen_first_of_repeated<string>(gen_text(1), 3);
  let c = gen_wrap_twice<float>(1.25);
  let d = gen_wrap_twice<GenColor>(GenColor.Green);
  `${a + 1} ${b} ${c.value.value + 1.0} ${c.count}${c.value.count} ${d.value.value}`
}

// A value is tested against a type parameter's type.
function gen_is<T>(x: unknown) -> bool {
  match (x) {
    let found: T => true,
    _ => false,
  }
}
function gen_count_of<T>(xs: unknown[]) -> int {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let n = 0;
  for (let x in xs) {
    gen_churn(1);
    if gen_is<T>(x) { n += 1; }
  };
  n
}
function gen_tests_its_type() -> string {
  let xs: unknown[] = [1, "a", 2.5, GenPoint { x: 1, y: 2 }, GenColor.Red, true, null, 3, gen_text(1)];
  `${gen_count_of<int>(xs)} ${gen_count_of<string>(xs)} ${gen_count_of<float>(xs)} ${gen_count_of<GenPoint>(xs)} ${gen_count_of<GenColor>(xs)} ${gen_count_of<bool>(xs)} ${gen_count_of<int | string>(xs)}`
}

// A class with a type parameter whose methods use what an interface gives
// a value of it: an order, an equality, a text.
interface GenShown {
  function shown(self) -> string throws never
}
implements GenShown for int {
  function shown(self) -> string throws never { `i${self}` }
}
implements GenShown for string {
  function shown(self) -> string throws never { `s${self}` }
}
implements GenShown for GenPoint {
  function shown(self) -> string throws never { `p${self.x},${self.y}` }
}
implements GenShown for GenColor {
  function shown(self) -> string throws never { `c${self}` }
}
implement baml.ops.Equals for GenPoint {
  function eq(self, other: Self) -> bool throws never { self.x == other.x }
}
implement baml.ops.Compare for GenPoint {
  function cmp(self, other: Self) -> baml.ops.Ordering throws never { self.x.cmp(other.x) }
}
function gen_largest<T extends baml.ops.Compare>(xs: T[]) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let best = xs[0];
  for (let x in xs) {
    if x.gt(best) { best = x; }
  };
  best
}
function gen_smallest<T extends baml.ops.Compare>(xs: T[]) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let best = xs[0];
  for (let x in xs) {
    if x < best { best = x; }
  };
  best
}
function gen_index_of<T extends baml.ops.Equals>(xs: T[], wanted: T) -> int {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  for (let i = 0; i < xs.length(); i += 1) {
    if xs[i].eq(wanted) { return i; }
  };
  -1
}
function gen_without<T extends baml.ops.Equals>(xs: T[], unwanted: T) -> int {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let n = 0;
  for (let x in xs) {
    if x.neq(unwanted) { n += 1; }
  };
  n
}
function gen_all_shown<T extends GenShown>(xs: T[]) -> string {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let out = "";
  for (let x in xs) { out = out + x.shown() + " "; };
  out
}
function gen_orders() -> string {
  let ints = [gen_count(3), 9, -4, 7];
  let floats = [1.5, -2.5, 8.25];
  let texts = [gen_text(1), "zz", "aa"];
  let flags = [false, true, false];
  let points = [GenPoint { x: 3, y: 0 }, GenPoint { x: 8, y: 1 }, GenPoint { x: -1, y: 2 }];
  `${gen_largest<int>(ints)} ${gen_smallest<int>(ints)} ${gen_largest<float>(floats)} ${gen_smallest<float>(floats)} ${gen_largest<string>(texts)} ${gen_smallest<string>(texts)} ${gen_largest<bool>(flags)} ${gen_largest<GenPoint>(points).y} ${gen_smallest<GenPoint>(points).y}`
}
function gen_equalities() -> string {
  let ints = [gen_count(3), 9, -4, 9];
  let floats = [1.5, -2.5, 8.25];
  let texts = [gen_text(1), "zz", gen_text(2)];
  let flags = [false, true, false];
  let points = [GenPoint { x: 3, y: 0 }, GenPoint { x: 8, y: 1 }, GenPoint { x: 8, y: 2 }];
  `${gen_index_of<int>(ints, 9)} ${gen_index_of<int>(ints, 5)} ${gen_index_of<float>(floats, 8.25)} ${gen_index_of<string>(texts, gen_text(2))} ${gen_index_of<bool>(flags, true)} ${gen_index_of<GenPoint>(points, GenPoint { x: 8, y: 9 })} ${gen_without<int>(ints, 9)} ${gen_without<string>(texts, "zz")} ${gen_without<GenPoint>(points, GenPoint { x: 8, y: 0 })}`
}
function gen_texts() -> string {
  let a = gen_all_shown<int>([1, gen_count(2)]);
  let b = gen_all_shown<string>([gen_text(1), "q"]);
  let c = gen_all_shown<GenPoint>([GenPoint { x: 1, y: 2 }]);
  let d = gen_all_shown<GenColor>([GenColor.Blue, GenColor.Red]);
  a + b + c + d
}

class GenSorted<T extends baml.ops.Compare> {
  items: T[]

  function add(self, item: T) -> int {
    let at = 0;
    while (at < self.items.length() && self.items[at].lt(item)) { at += 1; }
    self.items.push(item);
    let i = self.items.length() - 1;
    while (i > at) {
      self.items[i] = self.items[i - 1];
      i -= 1;
    }
    self.items[at] = item;
    at
  }
  function has(self, item: T) -> bool {
    for (let x in self.items) {
      if x.eq(item) { return true; }
    };
    false
  }
  function shown(self) -> string {
    let out = "";
    for (let x in self.items) { out = out + x.to_string() + " "; };
    out
  }
}
function gen_class_with_a_bound() -> string {
  let ints = GenSorted<int> { items: [] };
  ints.add(5); ints.add(gen_count(2)); ints.add(9); ints.add(-3);
  let texts = GenSorted<string> { items: [] };
  texts.add("pear"); texts.add(gen_text(1)); texts.add("fig");
  let floats = GenSorted<float> { items: [] };
  floats.add(2.5); floats.add(-1.0); floats.add(2.25);
  let points = GenSorted<GenPoint> { items: [] };
  points.add(GenPoint { x: 4, y: 1 }); points.add(GenPoint { x: -2, y: 2 }); points.add(GenPoint { x: 3, y: 3 });
  `${ints.shown()}${ints.has(9)} ${ints.has(4)} ${texts.shown()}${texts.has("fig")} ${floats.shown()}${floats.has(2.0)} ${points.items[0].y}${points.items[1].y}${points.items[2].y} ${points.has(GenPoint { x: 3, y: 0 })}`
}

// A map keyed by a type parameter's type.
function gen_tally<K extends baml.Hash>(keys: K[]) -> map<K, int> {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let seen: map<K, int> = {};
  for (let key in keys) {
    seen[key] = (seen.get(key) ?? 0) + 1;
    gen_churn(1);
  };
  seen
}
function gen_keys_of_a_parameter() -> string {
  let texts = gen_tally<string>(["a", gen_text(1), "a", "ab"]);
  let ints = gen_tally<int>([3, gen_count(3), 4]);
  let colors = gen_tally<GenColor>([GenColor.Red, GenColor.Blue, GenColor.Red]);
  `${texts["a"]} ${texts["ab"]} ${texts.length()} ${ints[3]} ${ints[4]} ${colors[GenColor.Red]} ${colors.length()}`
}

// A function with type parameters is a value: one function, called with
// what each type it is named with says.
function gen_apply_twice<T>(f: (T) -> T throws never, x: T) -> T {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  f(f(x))
}
function gen_doubled(x: int) -> int { x * 2 }
function gen_function_values() -> string {
  let of_ints: (int) -> int throws never = gen_same<int>;
  let of_texts: (string) -> string throws never = gen_same<string>;
  let of_points = gen_same<GenPoint>;
  let a = of_ints(gen_count(4)) + 1;
  let b = of_texts(gen_text(1)) + "!";
  let c = of_points(GenPoint { x: 2, y: 5 });
  let d = gen_apply_twice<int>(gen_doubled, 3);
  let e = gen_apply_twice<string>(of_texts, "s");
  `${a} ${b} ${c.y} ${d} ${e}`
}
function gen_passes_itself_on<T>(x: T) -> (T) -> T throws never {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  gen_same<T>
}
function gen_function_values_of_a_parameter() -> string {
  let f = gen_passes_itself_on<int>(0);
  let g = gen_passes_itself_on<string>("");
  `${f(7) + 1} ${g("t")}`
}

// A method of a class with type parameters, bound to an instance.
class GenStack<T> {
  items: T[]

  function push(self, item: T) -> int {
    let w = 0;
    for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
    self.items.push(item);
    self.items.length()
  }
  function top(self) -> T {
    let w = 0;
    for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
    self.items[self.items.length() - 1]
  }
}
function gen_bound_methods() -> string {
  let ints = GenStack<int> { items: [] };
  let texts = GenStack<string> { items: [] };
  let push_int = ints.push;
  let push_text = texts.push;
  let top_int = ints.top;
  let top_text = texts.top;
  push_int(gen_count(3));
  let n = push_int(8);
  push_text(gen_text(2));
  let m = push_text("z");
  `${n} ${top_int() + 1} ${m} ${top_text()} ${ints.top()} ${texts.items[0]}`
}

// A closure made in a function with type parameters holds a value of one.
function gen_keeper<T>(x: T) -> (int) -> T throws never {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let kept = x;
  (n) -> { gen_churn(n); kept }
}
function gen_counter<T>(xs: T[]) -> (T) -> int throws never {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let seen: T[] = xs;
  (x) -> { seen.push(x); seen.length() }
}
function gen_closures() -> string {
  let a = gen_keeper<int>(gen_count(6));
  let b = gen_keeper<string>(gen_text(3));
  let c = gen_keeper<float>(0.25);
  let d = gen_keeper<GenPoint?>(null);
  let ints = [1];
  let count = gen_counter<int>(ints);
  count(5);
  let n = count(6);
  ints[0] = ints[1] + ints[2];
  `${a(1) + 1} ${b(2)} ${c(1) * 2.0} ${d(1) == null} ${n} ${ints[0]}`
}

// A type made of a type parameter's is made when the program runs: the
// same one however often, and whatever was allocated since.
function gen_is_or_null<T>(x: unknown) -> bool {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  gen_is<T?>(x)
}
function gen_is_either<A, B>(x: unknown) -> bool {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  let before = gen_is<A | B>(x);
  let made = gen_repeat<int>(w, 40);
  gen_churn(30);
  let after = gen_is<B | A | string>(x);
  before && after && made.length() == 40
}
function gen_is_one_of<A, B>(x: unknown) -> bool {
  let w = 0;
  for (let i = 0; i < 2; i += 1) { w += gen_count(i); w += gen_churn(1) % 5; w -= i; if w > 1000 { w = 0; } };
  match (x) {
    let found: A | B | null => true,
    _ => false,
  }
}
function gen_types_made_running() -> string {
  let xs: unknown[] = [1, "a", 2.5, GenPoint { x: 1, y: 2 }, GenColor.Red, true, null, gen_text(1)];
  let out = "";
  for (let x in xs) {
    let a = if gen_is_or_null<int>(x) { "i" } else { "-" };
    let b = if gen_is_or_null<GenPoint>(x) { "p" } else { "-" };
    let c = if gen_is_either<int, GenColor>(x) { "e" } else { "-" };
    let d = if gen_is_either<float, bool>(x) { "f" } else { "-" };
    let e = if gen_is_one_of<GenPoint, float>(x) { "o" } else { "-" };
    let f = if gen_is_or_null<int | bool>(x) { "u" } else { "-" };
    out = out + a + b + c + d + e + f + " ";
  };
  out
}
