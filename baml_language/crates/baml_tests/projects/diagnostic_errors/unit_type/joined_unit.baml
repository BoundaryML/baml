// BEP-079: an implicit unit that joins another value.
//
// Where nothing is expected of it, a block with no final expression simply
// evaluates to `null`. That stops being so the moment its value joins one
// that is not unit: the branches of an `if`, the arms of a `match` or
// `catch`, the elements of a literal, or the returns of a lambda would
// otherwise produce a silent `null` among their values.

// `side` is declared in `implicit_unit.baml`.

function yes() -> bool { true }

function fails() -> int throws string { throw "no" }

function pick<T>(a: T, b: T) -> T { a }

function length_of<T>(xs: T[]) -> int { xs.length() }

// --- branches and arms ---

function if_else_join(c: bool) -> int {
  let x = if (c) { "big" } else { side(); };
  1
}

function if_let_join(v: int?) -> int {
  let x = if let n: int = v { n } else { side(); };
  1
}

function match_join(n: int) -> int {
  let x = match (n) {
    1 => "one",
    _ => { side(); },
  };
  1
}

function catch_join() -> int {
  let x = fails() catch (e) {
    _ => { side(); },
  };
  1
}

// Every implicit unit in the join is reported, however deep it sits.
function nested_join(a: bool, b: bool) -> int {
  let x = if (a) { 1 } else { if (b) { side(); } else { side(); } };
  1
}

function lone_if_join(a: bool, b: bool) -> int {
  let x = if (a) { 1 } else { if (b) { side(); } };
  1
}

// --- literals ---

function array_join() -> int {
  let xs = ["a", { side(); }];
  1
}

function map_join() -> int {
  let m = { "a": 1, "b": { side(); } };
  1
}

// --- a lambda's returns ---

function lambda_return_join() -> int {
  let f = (c: bool) -> { if (c) { return; } 5 };
  1
}

function lambda_tail_join() -> int {
  let f = (c: bool) -> { if (c) { return 5; } side(); };
  1
}

// --- `??` ---

function coalesce_join(v: int?) -> int {
  let x = v ?? { side(); };
  1
}

// --- a type still being inferred ---

// The branches of an `if` are not held to a type that is still a variable,
// so that the first cannot fix it for the second: the variable's solution
// judges the implicit unit all the same.
function inferred_return_join() -> int {
  let ys = [1, 2].map((x) -> { if (yes()) { x } else { side(); } });
  1
}

function inferred_argument_join() -> int {
  let x = pick(1, { side(); });
  1
}

// The type parameter has nothing to solve from, so it reports as `!error`:
// the block is refused either way.
function unsolved_callback_return() -> int {
  let ys = [1, 2].filter_map((x) -> { side(); });
  1
}

function unsolved_element() -> int {
  length_of({ side(); })
}

function conflicting_callback_returns() -> int {
  let x = pick(() -> { 1 }, () -> { side(); });
  1
}

// --- still fine: nothing but unit values join ---

function unit_joins(c: bool) -> null {
  let a = if (c) { null } else { side(); };
  let b = if (c) { side(); } else { side(); };
  let f = (early: bool) -> { if (early) { return; } side(); };
  let xs = [{ side(); }, { side(); }];
  a
}
