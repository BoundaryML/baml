// === HOF with its own throws + callback throws ===
// A function that both throws itself AND rethrows callback effects.
// The effective throws should be the UNION of both.

function apply_guarded(f: () -> int) -> int {
  let result = f()
  if (result < 0) { throw "negative result" }
  result
}

// Pure callback: function's own throw should still appear
function test_guarded_pure() -> int {
  apply_guarded(() -> int { 42 })
}

// Throwing callback: both the function's own throw and the callback's throw
function test_guarded_throwing() -> int {
  apply_guarded(() -> int { throw 99 })
}
