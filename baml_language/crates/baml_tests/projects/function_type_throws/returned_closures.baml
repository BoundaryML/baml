// === Returned closures: should default to throws never ===

// Returning a pure closure - should be fine
function make_pure() -> (() -> int) {
  return () -> int { 42 }
}

// Returning a closure with explicit throws
function make_thrower() -> (() -> int throws string) {
  return () -> int { throw "error" }
}

// Using a returned closure
function test_use_pure() -> int {
  let f = make_pure()
  f()
}

function test_use_thrower() -> int {
  let f = make_thrower()
  f()
}
