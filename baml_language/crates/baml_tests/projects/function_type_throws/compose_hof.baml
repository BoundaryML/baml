// === Compose HOF: nested higher-order functions with generics ===

// Compose two functions, propagating throws from both.
// The callbacks f and g are captured by the returned lambda rather than being
// directly invoked in the compose body, so they fire the unused effect var diagnostic.
// ERROR: callback parameter `f` has implicit effect polymorphism but is not directly invoked
// ERROR: callback parameter `g` has implicit effect polymorphism but is not directly invoked
function compose<A, B, C>(
  f: (A) -> B,
  g: (B) -> C
) -> (A) -> C {
  return (a: A) -> C { g(f(a)) }
}

// Pure composition
function test_compose_pure() -> (int) -> string {
  compose(
    (x: int) -> int { x * 2 },
    (y: int) -> string { "result" }
  )
}

// First function throws
function test_compose_first_throws() -> (int) -> string {
  compose(
    (x: int) -> int { throw "f failed" },
    (y: int) -> string { "result" }
  )
}

// Second function throws
function test_compose_second_throws() -> (int) -> string {
  compose(
    (x: int) -> int { x * 2 },
    (y: int) -> string { throw "g failed" }
  )
}

// Both functions throw different types
function test_compose_both_throw() -> (int) -> string {
  compose(
    (x: int) -> int { throw "string error" },
    (y: int) -> string { throw 42 }
  )
}
