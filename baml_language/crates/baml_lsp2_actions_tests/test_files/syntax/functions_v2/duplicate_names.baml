function Bar {
  input string
  output string
}

function Bar(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

// error: The function "Bar" cannot be re-defined because a function with that name already exists.
//   -->  functions_v2/duplicate_names.baml:1
//    |
//    |
//  1 | function Bar {
//    |
//   -->  functions_v2/duplicate_names.baml:6
//    |
//  5 |
//  6 | function Bar(a: string, b: int | bool) -> int {
//    |
// error: The function "Bar" cannot be re-defined because a function with that name already exists.
//   -->  functions_v2/duplicate_names.baml:6
//    |
//  5 |
//  6 | function Bar(a: string, b: int | bool) -> int {
//    |
//   -->  functions_v2/duplicate_names.baml:1
//    |
//    |
//  1 | function Bar {
//    |
// error: Invalid syntax for function "Bar". Use:
// function Bar(params...) -> ReturnType { ... }
//   -->  functions_v2/duplicate_names.baml:1
//    |
//    |
//  1 | function Bar {
//    |
// error: Error validating: Unknown field `input` in function
//   -->  functions_v2/duplicate_names.baml:2
//    |
//  1 | function Bar {
//  2 |   input string
//    |
// error: Error validating: Unknown field `output` in function
//   -->  functions_v2/duplicate_names.baml:3
//    |
//  2 |   input string
//  3 |   output string
//    |
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  functions_v2/duplicate_names.baml:1
//    |
//    |
//  1 | function Bar {
//    |

//----
//- diagnostics
// error: Old-style function syntax. Use: function Name(params...) -> ReturnType { ... }
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Old-style function syntax. Use: function Name(params...) -> ReturnType { ... }
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: unresolved name: input
//    ╭─[ functions_v2_duplicate_names.baml:2:3 ]
//    │
//  2 │   input string
//    │   ──┬──  
//    │     ╰──── unresolved name: input
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: string
//    ╭─[ functions_v2_duplicate_names.baml:2:9 ]
//    │
//  2 │   input string
//    │         ───┬──  
//    │            ╰──── unresolved name: string
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: output
//    ╭─[ functions_v2_duplicate_names.baml:3:3 ]
//    │
//  3 │   output string
//    │   ───┬──  
//    │      ╰──── unresolved name: output
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: unresolved name: string
//    ╭─[ functions_v2_duplicate_names.baml:3:10 ]
//    │
//  3 │   output string
//    │          ───┬──  
//    │             ╰──── unresolved name: string
//    │ 
//    │ Note: Error code: E0003
// ───╯
// error: type mismatch: expected baml.llm.Client, got () -> unknown throws never
//    ╭─[ functions_v2_duplicate_names.baml:6:46 ]
//    │
//  6 │ ╭─▶ function Bar(a: string, b: int | bool) -> int {
//    ┆ ┆   
//  9 │ ├─▶ }
//    │ │       
//    │ ╰─────── type mismatch: expected baml.llm.Client, got () -> unknown throws never
//    │     
//    │     Note: Error code: E0001
// ───╯
// error: type mismatch: expected baml.llm.Client, got () -> unknown throws never
//    ╭─[ functions_v2_duplicate_names.baml:4:2 ]
//    │
//  4 │ ╭─▶ }
//    ┆ ┆   
//  9 │ ├─▶ }
//    │ │       
//    │ ╰─────── type mismatch: expected baml.llm.Client, got () -> unknown throws never
//    │     
//    │     Note: Error code: E0001
// ───╯
