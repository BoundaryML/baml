// Parameters left to their defaults. A default that is a constant number
// is passed by the call itself; any other is put there by the callee.

function default_steps(n: int, by: int = 3, plus: float = 0.5, tag: string = "t") -> float {
  if (n <= 0) {
    return 0.0;
  }
  by * 1.0 + plus + tag.length() + default_steps(n - 1, by = by, plus = plus, tag = tag)
}
function default_below(n: int, floor: int = -4, drop: float = -0.25) -> float {
  if (n <= floor) {
    return drop;
  }
  drop + default_below(n - 1, floor = floor, drop = drop)
}
class DefaultCounter {
  base: int

  function bump(self, n: int, by: int = 10, scale: float = 2.0) -> float {
    if (n <= 0) {
      return self.base * scale;
    }
    by + self.bump(n - 1, by = by, scale = scale)
  }
}

function default_int_and_float() -> float { default_steps(count(3)) }
function default_int_given() -> float { default_steps(count(2), by = 5) }
function default_float_given() -> float { default_steps(count(1), plus = 1.5) }
function default_reference_given() -> float { default_steps(count(2), tag = "long") }
function default_negative() -> float { default_below(count(2)) + default_below(count(1), floor = 0) }
function default_of_a_method() -> float {
  let counter = DefaultCounter { base: count(3) };
  counter.bump(count(2)) + counter.bump(count(2), scale = 0.5)
}

// Every parameter is passed as a tag and a payload, and one left to its
// default has the tag no value has: so a default is of any type, and the
// callee puts it there, on every call that leaves the parameter out.
enum DefaultShade {
  Light
  Dark
}
class DefaultBox { size: int }

function default_kinds(n: int, i: int = count(7), f: float = halves(3), b: bool = true, s: string = "dflt", e: DefaultShade = DefaultShade.Dark, c: DefaultBox = DefaultBox { size: 3 }, o: int? = null, u: int | string = "u", xs: int[] = [1, 2]) -> string {
  let pad = rep("-", n);
  let last = match (u) {
    let k: int => `int ${k}`,
    let t: string => `string ${t}`,
  };
  `${pad}${i} ${f} ${b} ${s} ${e} ${c.size} ${o ?? -1} ${last} ${xs.length()}`
}
function default_of_every_kind() -> string {
  default_kinds(count(1))
    + default_kinds(count(2), i = count(3), b = false, e = DefaultShade.Light, o = count(4), xs = [count(5)])
    + default_kinds(count(3), f = halves(3), s = rep("s", 2), c = DefaultBox { size: count(9) }, u = count(6))
}

// Named arguments in another order than declared, and every subset of
// three.
function default_three(n: int, x: int = 1, y: string = "y", z: float = 0.5) -> string {
  let pad = rep(".", n);
  `${pad}${x}${y}${z};`
}
function default_every_subset() -> string {
  default_three(count(1))
    + default_three(count(1), x = count(2))
    + default_three(count(1), y = rep("b", 2))
    + default_three(count(1), z = halves(3))
    + default_three(count(1), y = rep("c", 1), x = count(3))
    + default_three(count(1), z = halves(5), x = count(4))
    + default_three(count(1), z = halves(7), y = rep("d", 3))
    + default_three(count(1), z = halves(9), y = rep("e", 2), x = count(5))
}

// A default that is a call runs in the callee, once for each call that
// leaves the parameter out, after the defaults before it, and may read a
// parameter before it.
function default_note(log: string[], what: string) -> int {
  log.push(what);
  log.length()
}
function default_calls(log: string[], a: int = default_note(log, "a"), b: int = default_note(log, "b") + a, c: string = rep("c", a + b)) -> string {
  let pad = rep("+", log.length());
  `${pad}${a} ${b} ${c}|`
}
function default_that_is_a_call() -> string {
  let log: string[] = [];
  let out = default_calls(log)
    + default_calls(log, b = count(10))
    + default_calls(log, a = count(1), c = "given")
    + default_calls(log, a = count(2), b = count(3), c = rep("g", 2));
  let noted = "";
  for (let what in log) { noted = noted + what; };
  out + noted
}

// A parameter a closure captures lives in a cell, which never holds an
// argument that was left out.
function default_captured(
  n: int,
  by: int = count(10),
  tag: string = "t",
  scale: float = halves(4),
) -> () -> string throws never {
  let pad = rep("c", n);
  () -> string { `${pad}${by} ${tag} ${scale}` }
}
function default_of_a_captured_parameter() -> string {
  default_captured(count(1))() + default_captured(count(2), by = count(3))() + default_captured(count(1), scale = halves(1), tag = rep("u", 2))()
}

// A default that collects while the parameters after it are still left
// out.
function default_churns(
  n: int,
  a: string = rep("a", default_churn(40)),
  b: DefaultBox = DefaultBox { size: a.length() },
  c: string = rep("c", b.size),
) -> string {
  `${n} ${a.length()} ${b.size} ${c.length()}`
}
function default_churn(n: int) -> int {
  let total = 0;
  for (let i = 0; i < n; i += 1) { total += rep("x", i % 7 + 20).length() - 19 - i % 7; };
  total
}
function default_collects_before_later_defaults() -> string {
  default_churns(count(1)) + "|" + default_churns(count(2), b = DefaultBox { size: count(5) })
}

// A call that names its callee is laid over the callee's parameters where
// it is compiled: the implementation of an interface method may have
// parameters with defaults the interface does not, and a call checked
// against the interface passes them as left out.
interface DefaultShape {
  function area(self, scale: int) -> int throws never
  function name(self, prefix: string) -> string throws never
}
class DefaultSquare {
  side: int
  implements DefaultShape {
    function area(self, scale: int, pad: int = count(1)) -> int throws never {
      if scale > 100 { return self.area(scale - 100, pad = pad + 1) };
      self.side * self.side * scale + pad
    }
    function name(self, prefix: string, suffix: string = "!", again: int = count(1)) -> string throws never {
      if again > 3 { return self.name(prefix, again = again - 3) };
      prefix + "square" + suffix + `${again}`
    }
  }
}
implements DefaultShape for int {
  function area(self, scale: int, pad: int = count(2)) -> int throws never {
    if scale > 100 { return self.area(scale - 100, pad = pad + 1) };
    self * scale + pad
  }
  function name(self, prefix: string, suffix: string = "#") -> string throws never {
    if self > 3 { return (self - 3).name(prefix) };
    prefix + `int ${self}` + suffix
  }
}
function default_measure<T extends DefaultShape>(shape: T, scale: int) -> string {
  `${shape.name("a ")} ${shape.area(scale)};`
}
function default_measure_often<T extends DefaultShape>(shape: T, scale: int) -> string {
  let out = "";
  for (let i = 0; i < scale; i += 1) { out = out + `${shape.name("a ")} ${shape.area(scale + i * 100)};`; };
  out
}
function default_known_implementation() -> string {
  let square = DefaultSquare { side: count(3) };
  default_measure(square, count(2)) + default_measure(count(5), count(3)) + default_measure_often(count(7), count(2)) + ` ${square.area(count(1))} ${square.area(count(1), pad = count(5))} ${square.name("the ", suffix = "?")} ${square.name("one ", again = count(5))}`
}
