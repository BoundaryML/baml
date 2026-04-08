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

// Function with omitted throws that still throws from its body.
function helper_with_body_throw() -> int {
  throw "helper boom"
}

// Wrapper should keep the helper's concrete body throw alongside callback effects.
function apply_with_helper(f: () -> int) -> int {
  helper_with_body_throw()
  f()
}

function test_apply_with_helper_pure() -> int {
  apply_with_helper(() -> int { 42 })
}

function test_apply_with_helper_throwing() -> int {
  apply_with_helper(() -> int { throw 42 })
}
