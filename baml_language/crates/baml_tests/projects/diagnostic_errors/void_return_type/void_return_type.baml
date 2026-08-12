// Basic void function — empty body
function do_nothing() -> void {
}

// Void function with bare return
function do_return() -> void {
  return;
}

// ERROR: return with value in void function
function do_bad_return() -> void {
  return 42;
}

// ERROR: assigning void function result
function use_void() -> int {
  let x = do_nothing();
  1
}

// Bare call as statement — should be fine
function call_void() -> int {
  do_nothing();
  1
}

// Void function with side effects (statements, no tail)
function side_effects(x: int) -> void {
  let y = x + 1;
  y;
}
