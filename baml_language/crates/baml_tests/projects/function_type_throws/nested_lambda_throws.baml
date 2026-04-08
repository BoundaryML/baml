// === Nested lambda throws behavior ===

// Inner lambda throws, outer doesn't throw directly
function test_nested_inner_throws() -> int {
  let outer = () -> int {
    let inner = () -> int { throw "inner boom" }
    inner()
  }
  outer()
}

// Outer lambda throws, inner is pure
function test_nested_outer_throws() -> int {
  let outer = () -> int {
    let inner = () -> int { 42 }
    throw "outer boom"
  }
  outer()
}

// Both throw different types
function test_nested_both_throw() -> int {
  let outer = () -> int {
    let inner = () -> int { throw 42 }
    inner()
    throw "outer"
  }
  outer()
}
