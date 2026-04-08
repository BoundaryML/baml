// === Mixed: function with both function and non-function params ===

// Only the function param should get an effect var
function apply_with_arg(x: int, f: (int) -> int) -> int {
  f(x)
}

function test_mixed_pure() -> int {
  apply_with_arg(5, (n: int) -> int { n * 2 })
}

function test_mixed_throwing() -> int {
  apply_with_arg(5, (n: int) -> int {
    if (n < 0) { throw "negative" }
    n * 2
  })
}

// Multiple non-function params with one callback
function apply_with_many_args(a: int, b: string, f: (int, string) -> string) -> string {
  f(a, b)
}

function test_many_args_pure() -> string {
  apply_with_many_args(1, "hello", (n: int, s: string) -> string { s })
}
