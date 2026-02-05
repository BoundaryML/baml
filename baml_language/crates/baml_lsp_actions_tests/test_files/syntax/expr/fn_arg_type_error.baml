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
// Error: Expected `4`, found `3`
//    ╭─[ expr_fn_arg_type_error.baml:9:18 ]
//    │
//  9 │   let y = Takes4(3);
//    │                  ┬  
//    │                  ╰── Expected `4`, found `3`
//    │ 
//    │ Note: Error code: E0001
// ───╯
