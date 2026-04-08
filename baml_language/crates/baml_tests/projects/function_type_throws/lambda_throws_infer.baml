// === Lambda throws inference ===

// Pure lambda - should infer throws: never
function test_pure_lambda() -> int {
  let f = () -> int { 42 }
  f()
}

// Throwing lambda - should infer throws: string
function test_throwing_lambda() -> int {
  let f = () -> int { throw "boom" }
  f()
}

// Lambda throwing int - should infer throws: int
function test_throwing_int() -> string {
  let f = () -> string { throw 42 }
  f()
}

// Lambda with conditional throw - only one branch throws
function test_conditional_throw(x: int) -> int {
  let f = (n: int) -> int {
    if (n < 0) { throw "negative" }
    n
  }
  f(x)
}

// Lambda with multiple throw types - should infer throws: string | int
function test_multi_throw_types(x: int) -> string {
  let f = (n: int) -> string {
    match (n) {
      0 => throw "string error",
      1 => throw 1,
      _ => "ok"
    }
  }
  f(x)
}
