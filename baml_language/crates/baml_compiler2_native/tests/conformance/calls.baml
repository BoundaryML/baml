// Calls. Every argument and every result is passed as a tag and a payload,
// whatever its type, so a function is entered the same way by a call that
// names it, through a function value and through an interface, and one
// function type stands for another the type checker lets it stand for.

class CallThing { size: int }
enum CallShade {
  Light
  Dark
}

function call_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function call_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += rep("x", i % 7 + 20).length(); };
  total
}
function call_either(i: int) -> int | string {
  if i % 2 == 0 { i } else { rep("s", i) }
}
function call_maybe(n: int) -> int? {
  if n % 2 == 0 { n } else { null }
}

// A function that takes more than its type says it is given, and one that
// returns less than its type says it may.
function call_wide(x: int | string) -> int {
  match (x) {
    let n: int => n + 1,
    let s: string => s.length(),
  }
}
function call_narrow(x: int) -> int { x * 2 }
function call_apply(f: (int) -> int throws never, x: int) -> int { f(x) }
function call_read(f: (int) -> int | string throws never, x: int) -> string {
  match (f(x)) {
    let n: int => `int ${n}`,
    let s: string => `string ${s}`,
  }
}
function call_variance_of_an_argument() -> int {
  call_apply(call_wide, call_count(4)) * 100 + call_apply(call_narrow, call_count(4))
}
function call_variance_of_a_result() -> string {
  call_read(call_narrow, call_count(4)) + "," + call_read(call_either, call_count(3)) + "," + call_read(call_wide, call_count(6))
}
function call_variance_in_an_array() -> string {
  let takers: ((int) -> int throws never)[] = [];
  takers.push(call_wide);
  takers.push(call_narrow);
  takers.push((x: int) -> int { x - 1 });
  let givers: ((int) -> int | string throws never)[] = [];
  givers.push(call_narrow);
  givers.push(call_either);
  givers.push(call_wide);
  let out = "";
  for (let f in takers) { out = out + `${f(call_count(10))},`; };
  for (let f in givers) {
    out = out + match (f(call_count(5))) {
      let n: int => `int ${n},`,
      let s: string => `string ${s},`,
    };
  };
  out
}
class CallHolder {
  take: (int) -> int throws never
  give: (int) -> int | string throws never
}
function call_variance_in_a_field() -> string {
  let holders: CallHolder[] = [];
  holders.push(CallHolder { take: call_wide, give: call_narrow });
  holders.push(CallHolder { take: call_narrow, give: call_either });
  let out = "";
  for (let holder in holders) {
    out = out + `${(holder.take)(call_count(7))}/` + match ((holder.give)(call_count(7))) {
      let n: int => `int ${n},`,
      let s: string => `string ${s},`,
    };
  };
  out
}

// Five, seven and nine parameters of every kind: past the third the words
// of a value are passed on the stack.
function call_five(a: int, b: float, c: bool, d: string, e: int?) -> string {
  `${a} ${b} ${c} ${d.length()} ${e ?? -1}`
}
function call_seven(a: int, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing) -> string {
  let sixth = match (f) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  `${a} ${b} ${c} ${d.length()} ${e ?? -1} ${sixth} ${g.size}`
}
function call_nine(a: int, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing, h: string?, i: float) -> float {
  let sixth = match (f) {
    let n: int => n,
    let s: string => s.length(),
  };
  a + b + (if c { 100.0 } else { 0.0 }) + d.length() + (e ?? -1) + sixth + g.size + (h ?? "none").length() + i
}
function call_many_directly() -> string {
  call_five(call_count(1), halves(3), call_count(1) == 1, rep("d", 4), call_maybe(6))
    + "|" + call_seven(call_count(2), halves(5), call_count(1) == 0, rep("d", 5), call_maybe(7), call_either(8), CallThing { size: 9 })
    + "|" + call_seven(call_count(3), halves(7), true, rep("d", 6), call_maybe(8), call_either(9), CallThing { size: 10 })
    + `|${call_nine(call_count(4), halves(1), false, rep("d", 7), call_maybe(9), call_either(11), CallThing { size: 12 }, null, halves(3))}`
    + `|${call_nine(call_count(5), halves(2), true, rep("d", 8), call_maybe(10), call_either(12), CallThing { size: 13 }, rep("h", 14), halves(5))}`
}
function call_many_through_values() -> string {
  let fives = [call_five];
  let sevens = [call_seven];
  let nines = [call_nine];
  fives[call_count(0)](call_count(1), halves(3), call_count(1) == 1, rep("d", 4), call_maybe(6))
    + "|" + sevens[call_count(0)](call_count(2), halves(5), call_count(1) == 0, rep("d", 5), call_maybe(7), call_either(8), CallThing { size: 9 })
    + `|${nines[call_count(0)](call_count(4), halves(1), false, rep("d", 7), call_maybe(9), call_either(11), CallThing { size: 12 }, null, halves(3))}`
    + `|${nines[call_count(0)](call_count(5), halves(2), true, rep("d", 8), call_maybe(10), call_either(12), CallThing { size: 13 }, rep("h", 14), halves(5))}`
}
interface CallWide {
  function five(self, b: float, c: bool, d: string, e: int?) -> string throws never
  function seven(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing) -> string throws never
  function nine(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing, h: string?, i: float) -> float throws never
}
implements CallWide for int {
  function five(self, b: float, c: bool, d: string, e: int?) -> string throws never { call_five(self, b, c, d, e) }
  function seven(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing) -> string throws never { call_seven(self, b, c, d, e, f, g) }
  function nine(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing, h: string?, i: float) -> float throws never { call_nine(self, b, c, d, e, f, g, h, i) }
}
implements CallWide for CallThing {
  function five(self, b: float, c: bool, d: string, e: int?) -> string throws never { `thing ${self.size} ${b} ${c} ${d.length()} ${e ?? -1}` }
  function seven(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing) -> string throws never { "thing " + call_seven(self.size, b, c, d, e, f, g) }
  function nine(self, b: float, c: bool, d: string, e: int?, f: int | string, g: CallThing, h: string?, i: float) -> float throws never { 1000.0 + call_nine(self.size, b, c, d, e, f, g, h, i) }
}
function call_pick(i: int) -> CallWide {
  if i % 2 == 0 { call_count(i) } else { CallThing { size: i } }
}
function call_many_through_an_interface() -> string {
  let out = "";
  for (let i = 0; i < 4; i += 1) {
    let on = call_pick(i);
    out = out + on.five(halves(i), i == 1, rep("d", i), call_maybe(i))
      + "|" + on.seven(halves(i), i == 2, rep("d", i + 1), call_maybe(i + 1), call_either(i), CallThing { size: i + 20 })
      + `|${on.nine(halves(i), i == 3, rep("d", i + 2), call_maybe(i), call_either(i + 1), CallThing { size: i + 30 }, if i < 2 { null } else { rep("h", i) }, halves(i + 1))};`;
  };
  out
}

// A `float` goes through a function value as any value does.
function call_half(x: float) -> float { x / 2.0 }
function call_floats_through_values() -> string {
  let fs: ((float) -> float throws never)[] = [];
  fs.push(call_half);
  fs.push((x: float) -> float { x * 1.5 + halves(1) });
  let mixed: ((float, int) -> float | int throws never)[] = [];
  mixed.push((x: float, n: int) -> float { x + n });
  mixed.push((x: float, n: int) -> int { n * 2 });
  let out = "";
  for (let f in fs) { out = out + `${f(halves(5))},`; };
  for (let f in mixed) {
    out = out + match (f(halves(3), call_count(4))) {
      let n: int => `int ${n},`,
      let x: float => `float ${x},`,
    };
  };
  out
}

// An argument on the stack is the only reference to what it refers to,
// and the callee collects before it reads it.
function call_late(a: int, b: int, c: int, d: string, e: CallThing, f: string[], g: string?, h: int | string) -> string {
  let churned = call_churn(100);
  let last = match (h) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  `${a + b + c} ${d.length()} ${e.size} ${f.length()} ${f[1]} ${(g ?? "none").length()} ${last} ${churned}`
}
function call_strings(n: int) -> string[] {
  let out: string[] = [];
  for (let i = 0; i < n; i += 1) { out.push(rep("e", i)); };
  out
}
function call_collects_under_stack_arguments() -> string {
  call_late(call_count(1), call_count(2), call_count(3), rep("d", 40), CallThing { size: 41 }, call_strings(3), rep("g", 42), call_either(43))
    + "|" + call_late(call_count(4), call_count(5), call_count(6), rep("0123456789", 5).slice(10, 40), CallThing { size: 44 }, call_strings(5), null, call_either(44))
}

// A throw out of a callee with arguments on the stack, caught by its
// caller, which goes on with what it held.
function call_fails(a: int, b: string, c: int | string, d: string, e: CallThing, f: string) -> string throws string {
  let churned = call_churn(10);
  if a > 0 { throw `${b}${d}${e.size}${f}${churned}` }
  `${b.length()} ${d.length()} ${f.length()}`
}
function call_throws_past_stack_arguments() -> string {
  let held = rep("k", 5);
  let thing = CallThing { size: 6 };
  let out = "";
  for (let i = 0; i < 3; i += 1) {
    let text = call_fails(i % 2, rep("b", 2), call_either(i), rep("d", 3), thing, rep("f", 4)) catch (e) {
      string => `caught ${e}`,
    };
    out = out + text + held + `${thing.size},`;
  };
  out
}

// A deep recursion of a function with arguments on the stack, which
// allocates at every depth, returns through every frame, and throws
// through every frame.
function call_dive(depth: int, a: string, b: int | string, c: CallThing, d: float, e: string?, f: int) -> int throws string {
  if depth == 0 {
    if f < 0 { throw a + (e ?? "none") }
    return a.length() + c.size + f;
  }
  let own = rep("o", depth % 5 + 1);
  let below = call_dive(depth - 1, own, b, CallThing { size: c.size + 1 }, d + 0.5, if depth % 2 == 0 { own } else { null }, f);
  let second = match (b) {
    let n: int => n,
    let s: string => s.length(),
  };
  below + own.length() + a.length() + second + (e ?? "").length()
}
function call_recursion_with_stack_arguments() -> string {
  let down = call_dive(300, rep("a", 3), call_either(7), CallThing { size: 1 }, halves(1), null, call_count(2)) catch (e) {
    string => -1,
  };
  let thrown = { `${call_dive(300, rep("a", 3), call_either(8), CallThing { size: 1 }, halves(1), null, 0 - call_count(2))}` } catch (e) {
    string => `threw ${e}`,
  };
  `${down} ${thrown}`
}

// An enum variant, a `bool` and an optional left to their defaults.
function call_defaulted(n: int, shade: CallShade = CallShade.Dark, flag: bool = true, maybe: int? = null, thing: CallThing? = null, name: string = "name") -> string {
  let size = if thing == null { -1 } else { n };
  `${n} ${shade == CallShade.Dark} ${flag} ${maybe ?? -1} ${size} ${name}`
}
function call_defaults_of_every_kind() -> string {
  call_defaulted(call_count(1))
    + "|" + call_defaulted(call_count(2), shade = CallShade.Light)
    + "|" + call_defaulted(call_count(3), flag = false, maybe = call_count(4))
    + "|" + call_defaulted(call_count(5), thing = CallThing { size: 6 }, name = rep("n", 2))
}
