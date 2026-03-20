// Test: Literal type mismatch preserves both literal types in error

function Takes4(x: 4) -> int {
  1
}

function TestLiteralMismatch() -> int {
  // Should show "Expected `4`, found `3`" (not "found `int`")
  let y = Takes4(3);
  1
}

//----
//- diagnostics
// <no-diagnostics-expected>
