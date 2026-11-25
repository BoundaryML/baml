// Call layouts. A function with parameters that have defaults stands where
// a function type with fewer of them is expected, so a call through a
// function value compares the layout it pushes with the callee's own, and
// goes through a thunk that lays its arguments out again where they
// differ. Each parameter the call does not have is left to its default,
// which the callee puts there.

function layout_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}
function layout_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += rep("x", i % 7 + 20).length(); };
  total
}

// A function with one more parameter than the type it is used at.
function layout_f(a: int, extra: int = layout_count(2)) -> int throws never {
  if a > 100 { return layout_f(a - 100, extra = extra + 1) };
  a + extra * 10
}
function layout_fewer_parameters_than_the_function() -> string {
  let g: (int) -> int throws never = layout_f;
  `${g(layout_count(1))} ${g(layout_count(203))}`
}

// Two functions that share one named parameter, at another place in each.
function layout_foo(a: int, x: int = layout_count(1), y: int = layout_count(2)) -> int throws never {
  if a > 100 { return layout_foo(a - 100, y = y) };
  a * 100 + x * 10 + y
}
function layout_bar(a: int, y: int = layout_count(3), z: int = layout_count(4)) -> int throws never {
  if a > 100 { return layout_bar(a - 100, y = y) };
  a * 100 + y * 10 + z
}
function layout_shared_named_parameter() -> string {
  let fs: ((int, y?: int) -> int throws never)[] = [layout_foo, layout_bar];
  let out = `${fs[0](1, y = 7)} ${fs[1](1)}`;
  for (let f in fs) { out = out + ` ${f(layout_count(2))} ${f(layout_count(3), y = layout_count(8))} ${f(layout_count(204), y = layout_count(9))}`; };
  out
}

// One call, reached by a lambda, which has the call's own layout, and by
// functions with defaults, which do not.
function layout_apply(f: (int) -> int throws never, x: int) -> int {
  let total = 0;
  for (let i = 0; i < 2; i += 1) { total += f(x + i); };
  total
}
function layout_one_call_of_several_layouts() -> string {
  let fs: ((int) -> int throws never)[] = [];
  fs.push((n: int) -> int { n + 1 });
  fs.push(layout_f);
  fs.push(layout_foo);
  fs.push(layout_bar);
  let out = `${layout_apply((n: int) -> int { n * 2 }, layout_count(5))} ${layout_apply(layout_f, layout_count(5))}`;
  for (let f in fs) { out = out + ` ${f(layout_count(6))} ${layout_apply(f, layout_count(7))}`; };
  out
}

// A bound method's layout is its method's after the receiver.
class LayoutCounter {
  base: int

  function bump(self, n: int, by: int = layout_count(10), scale: int = layout_count(2)) -> int throws never {
    if n > 100 { return self.bump(n - 100, by = by, scale = scale + 1) };
    (self.base + n + by) * scale
  }
}
function layout_bumps(counters: LayoutCounter[]) -> string {
  let out = "";
  for (let counter in counters) {
    let plain: (int) -> int throws never = counter.bump;
    let named: (int, scale?: int) -> int throws never = counter.bump;
    let all: (int, scale?: int, by?: int) -> int throws never = counter.bump;
    out = out + `${plain(layout_count(2))} ${named(layout_count(2))} ${named(layout_count(2), scale = layout_count(3))} ${all(layout_count(102), by = layout_count(1))} ${all(layout_count(1), by = layout_count(4), scale = layout_count(5))},`;
  };
  out
}
function layout_of_a_bound_method() -> string {
  let counters: LayoutCounter[] = [];
  counters.push(LayoutCounter { base: layout_count(1) });
  counters.push(LayoutCounter { base: layout_count(20) });
  layout_bumps(counters)
}

// A function value in a field, and passed through generic code.
class LayoutHolder {
  run: (int) -> int throws never
  named: (int, y?: int) -> int throws never
}
function layout_twice<T>(f: (T) -> T throws never, x: T) -> T {
  let once = f(x);
  f(once)
}
function layout_in_a_field_and_through_generic_code() -> string {
  let holders: LayoutHolder[] = [];
  holders.push(LayoutHolder { run: layout_f, named: layout_foo });
  holders.push(LayoutHolder { run: layout_bar, named: layout_bar });
  holders.push(LayoutHolder { run: (n: int) -> int { n - 1 }, named: layout_foo });
  let out = "";
  for (let holder in holders) {
    out = out + `${(holder.run)(layout_count(1))} ${(holder.named)(layout_count(2), y = layout_count(9))} ${(holder.named)(layout_count(2))} ${layout_twice(holder.run, layout_count(3))},`;
  };
  out
}

// More parameters than the registers hold, so the thunk moves words of
// the stack, and a callee that collects before it reads them.
function layout_wide(
  a: int,
  b: string,
  c: LayoutCounter,
  d: string[],
  e: int | string,
  f: float = halves(3),
  g: string = rep("g", 3),
  h: LayoutCounter = LayoutCounter { base: 5 },
) -> string throws never {
  let churned = layout_churn(100);
  let fifth = match (e) {
    let n: int => `int ${n}`,
    let s: string => `string ${s.length()}`,
  };
  `${a} ${b.length()} ${c.base} ${d.length()} ${fifth} ${f} ${g} ${h.base} ${churned};`
}
function layout_strings(n: int) -> string[] {
  let out: string[] = [];
  for (let i = 0; i < n; i += 1) { out.push(rep("e", i)); };
  out
}
function layout_either(i: int) -> int | string {
  if i % 2 == 0 { i } else { rep("s", i) }
}
function layout_stack_arguments_through_a_thunk() -> string {
  let narrow: (int, string, LayoutCounter, string[], int | string) -> string throws never = layout_wide;
  let some: (int, string, LayoutCounter, string[], int | string, h?: LayoutCounter, g?: string) -> string throws never = layout_wide;
  narrow(layout_count(1), rep("b", 30), LayoutCounter { base: 31 }, layout_strings(3), layout_either(4))
    + narrow(layout_count(2), rep("0123456789", 4).slice(5, 30), LayoutCounter { base: 32 }, layout_strings(4), layout_either(5))
    + some(layout_count(3), rep("b", 33), LayoutCounter { base: 34 }, layout_strings(5), layout_either(6), g = rep("x", 2))
    + some(layout_count(4), rep("b", 35), LayoutCounter { base: 36 }, layout_strings(6), layout_either(7), h = LayoutCounter { base: 37 }, g = rep("0123456789", 2).slice(3, 9))
    + some(layout_count(5), rep("b", 38), LayoutCounter { base: 39 }, layout_strings(7), layout_either(8), h = LayoutCounter { base: 40 })
}

// A throw out of a callee entered through a thunk, caught by the caller,
// which goes on with what it held.
function layout_fails(a: int, why: string = rep("w", 2), held: string[] = layout_strings(3)) -> int throws string {
  let churned = layout_churn(10);
  if a > 100 { return layout_fails(a - 100, why = why, held = held) + 1 }
  if a > 0 { throw `${why}${a}${held.length()}${churned}` }
  a - 1
}
function layout_throw_through_a_thunk() -> string {
  let g: (int) -> int throws string = layout_fails;
  let named: (int, held?: string[]) -> int throws string = layout_fails;
  let kept = rep("k", 5);
  let first = g(layout_count(4)) catch (e) {
    string => e.length(),
  };
  let second = named(layout_count(3), held = layout_strings(9)) catch (e) {
    string => e.length() * 100,
  };
  let third = g(layout_count(0)) catch (e) {
    string => -100,
  };
  let fourth = g(layout_count(202)) catch (e) {
    string => e.length() * 10000,
  };
  `${kept} ${first} ${second} ${third} ${fourth}`
}

// The thunk is no frame of a trace.
function layout_trace_through_a_thunk() -> string {
  let g: (int) -> int throws string = layout_fails;
  let failed = g(layout_count(101)) catch (e, context) {
    _ => context.to_string(),
  };
  `${failed}`
}

// A task's closure is called with no argument: a function whose every
// parameter has a default is entered through a thunk there too.
function layout_job(n: int = layout_count(20), tag: string = rep("j", 2)) -> int throws never {
  if n > 100 { return layout_job(n = n - 100, tag = tag) };
  n + tag.length()
}
class LayoutRuns {
  body: () -> int throws never

  implements baml.spawn.Modifier<int, never> {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
      baml.spawn.Plan<int, never> { body: self.body, name: plan.name, semaphores: plan.semaphores, cancel_tokens: plan.cancel_tokens, root: plan.root }
    }
  }
}
function layout_start(
  body: () -> int throws never,
) -> int {
  let started = spawn with LayoutRuns { body: body } { 0 };
  await started
}
function layout_of_a_spawned_function() -> string {
  let bodies: (() -> int throws never)[] = [];
  bodies.push(layout_job);
  bodies.push(() -> int { layout_count(7) });
  let out = "";
  for (let body in bodies) { out = out + `${layout_start(body)} `; };
  let named = spawn with LayoutRuns { body: layout_job } { 0 };
  let plain = spawn with LayoutRuns { body: () -> int { layout_count(9) } } { 0 };
  `${out}${await named} ${await plain}`
}

// An interface's method has no parameter with a default, and an
// implementation may add some. A call that dispatches the method pushes
// the interface's slots and checks nothing: the row of a type whose
// implementation declares more holds a thunk that leaves them out.
interface LayoutShape {
  function area(self, scale: int) -> int throws never
  function label(self, prefix: string, n: int) -> string throws never
}
class LayoutSquare {
  side: int

  implements LayoutShape {
    function area(self, scale: int, pad: int = layout_count(1)) -> int throws never {
      if scale > 100 { return self.area(scale - 100, pad = pad + 1) };
      self.side * self.side * scale + pad
    }
    function label(self, prefix: string, n: int, suffix: string = rep("!", 2), again: int = layout_count(1)) -> string throws never {
      if again > 3 { return self.label(prefix, n, again = again - 3) };
      `${prefix}square ${self.side} ${n}${suffix}${again}`
    }
  }
}
class LayoutCircle {
  radius: int

  implements LayoutShape {
    function area(self, scale: int) -> int throws never { self.radius * self.radius * 3 * scale }
    function label(self, prefix: string, n: int) -> string throws never { `${prefix}circle ${self.radius} ${n}` }
  }
}
implements LayoutShape for int {
  function area(self, scale: int, pad: int = layout_count(2), more: int = layout_count(3)) -> int throws never {
    if scale > 100 { return self.area(scale - 100, more = more + 1) };
    self * scale + pad * 10 + more
  }
  function label(self, prefix: string, n: int, suffix: string = rep("#", 1)) -> string throws never {
    if self > 3 { return (self - 3).label(prefix, n) };
    `${prefix}int ${self} ${n}${suffix}`
  }
}
implements LayoutShape for string {
  function area(self, scale: int, pad: string = rep("p", 4), held: string[] = layout_strings(2)) -> int throws never {
    let churned = layout_churn(20);
    self.length() * scale + pad.length() + held.length() + churned
  }
  function label(self, prefix: string, n: int) -> string throws never { `${prefix}string ${self} ${n}` }
}
function layout_shape(i: int) -> LayoutShape {
  if i % 4 == 0 { LayoutSquare { side: layout_count(i + 2) } }
  else if i % 4 == 1 { LayoutCircle { radius: layout_count(i + 1) } }
  else if i % 4 == 2 { layout_count(i + 3) }
  else { rep("s", i) }
}
function layout_measure<T extends LayoutShape>(shape: T, scale: int) -> string {
  let out = "";
  for (let i = 0; i < 2; i += 1) { out = out + `${shape.label("a ", i)} ${shape.area(scale + i * 100)};`; };
  out
}
function layout_dispatch_to_more_parameters() -> string {
  let out = "";
  for (let i = 0; i < 8; i += 1) {
    let shape = layout_shape(i);
    out = out + `${shape.label("the ", i)} ${shape.area(layout_count(i))} ${shape.area(layout_count(i) + 200)},`;
  };
  out
}
function layout_dispatch_in_generic_code() -> string {
  layout_measure(LayoutSquare { side: layout_count(3) }, layout_count(2))
    + layout_measure(LayoutCircle { radius: layout_count(2) }, layout_count(3))
    + layout_measure(layout_count(5), layout_count(4))
    + layout_measure(rep("t", 6), layout_count(5))
}
