// BEP-079: a value where only the unit type is admitted.
//
// A unit-returning body is checked like any other body: its final expression
// is its value. A non-unit value there would be dropped silently, so the
// author discards it with `;`.

function produce() -> int { 1 }

function tail_in_void() -> void {
  produce()
}

function tail_in_null() -> null {
  produce()
}

function nested_tail() -> void {
  {
    produce()
  }
}

function lambda_tail() -> int {
  let f = () -> void { produce() };
  f();
  1
}

function match_arm_tail(x: int) -> void {
  match (x) {
    1 => { produce() },
    _ => null,
  }
}

function if_else_tail(c: bool) -> void {
  if (c) { produce() } else { produce(); }
}

// A block annotated with the unit type is no different from a body.
function annotated_block() -> int {
  let u: void = { produce() };
  1
}

test "a test body is a unit body" {
  produce()
}

// Not the final expression of a block: an ordinary mismatch, with no `;`
// to suggest.
function return_value_in_void() -> void {
  return 42;
}

function annotated_value() -> int {
  let u: void = 1;
  2
}

// An `if` with no `else` evaluates to the unit value, so its branch must be
// unit too — whether the `if` is a unit body's value or bound to a name.
function lone_if_tail(c: bool) -> void {
  if (c) { produce() }
}

function lone_if_binding(c: bool) -> int {
  let x = if (c) { produce() };
  1
}

function lone_if_let_binding(v: int?) -> int {
  let x = if let n: int = v { n };
  1
}

// A value typed by a block's own `type T = …` is no different: where the unit
// type is expected it is a value left undiscarded, not one that escapes.
function make<T>() -> T[] { [] }

function scoped_tail(t: reflect.Type) -> void {
  type T = unreflect(t);
  make<T>()
}
