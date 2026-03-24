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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: type mismatch: expected 4, got 3
//    ╭─[ expr_fn_arg_type_error.baml:9:18 ]
//    │
//  9 │   let y = Takes4(3);
//    │                  ┬  
//    │                  ╰── type mismatch: expected 4, got 3
//    │ 
//    │ Note: Error code: E0001
// ───╯
