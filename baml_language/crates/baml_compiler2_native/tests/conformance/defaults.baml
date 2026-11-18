// Parameters left to their defaults. An `int` or a `float` has no word that
// says "omitted", so a call passes the default itself.

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
