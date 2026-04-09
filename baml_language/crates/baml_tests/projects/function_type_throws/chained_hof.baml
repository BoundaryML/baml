// === Chained HOF: throws through multiple layers ===

function apply_inner(f: () -> int) -> int { f() }

// ERROR: callback parameter `g` has implicit effect polymorphism but is not directly invoked
function apply_outer(g: () -> int) -> int {
  apply_inner(g)
}

// Pure through two layers
function test_chained_pure() -> int {
  apply_outer(() -> int { 42 })
}

// Throwing through two layers
function test_chained_throwing() -> int {
  apply_outer(() -> int { throw "deep" })
}
