// === Higher-order function throws propagation ===

// Function taking a callback and calling it
function apply(f: () -> int) -> int {
  f()
}

// Calling apply with a pure lambda
function test_apply_pure() -> int {
  apply(() -> int { 42 })
}

// Calling apply with a throwing lambda
function test_apply_throwing() -> int {
  apply(() -> int { throw "boom" })
}

// Function taking callback with explicit throws
function apply_throwing(f: () -> int throws string) -> int {
  f()
}

function test_apply_explicit_throws() -> int {
  apply_throwing(() -> int { throw "boom" })
}

// Function that takes a callback and calls it, with its own throw
function apply_and_throw(f: () -> int) -> int {
  let result = f()
  if (result < 0) { throw "negative result" }
  result
}

function test_apply_and_throw_pure() -> int {
  apply_and_throw(() -> int { 42 })
}
