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
// Error: Expected '(', found '{'
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Expected '(', found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found '{'
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Expected parameter name, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found '{'
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Expected type annotation, found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found '{'
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Expected ')', found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected return type (->), found '{'
//    ╭─[ functions_v2_duplicate_names.baml:1:14 ]
//    │
//  1 │ function Bar {
//    │              ┬  
//    │              ╰── Expected return type (->), found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Duplicate function `Bar`
//    ╭─[ functions_v2_duplicate_names.baml:1:1 ]
//    │
//  1 │ function Bar {
//    │ │ 
//    │ ╰─ function `Bar` redefined here
//    │ │ 
//    │ ╰─ `Bar` first defined in functions_v2_duplicate_names.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:2:3 ]
//    │
//  2 │   input string
//    │   ──┬──  
//    │     ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:2:9 ]
//    │
//  2 │   input string
//    │         ───┬──  
//    │            ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:3:3 ]
//    │
//  3 │   output string
//    │   ───┬──  
//    │      ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Unknown variable `input`
//    ╭─[ functions_v2_duplicate_names.baml:2:3 ]
//    │
//  2 │   input string
//    │   ──┬──  
//    │     ╰──── Unknown variable `input`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `string`
//    ╭─[ functions_v2_duplicate_names.baml:2:9 ]
//    │
//  2 │   input string
//    │         ───┬──  
//    │            ╰──── Unknown variable `string`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `output`
//    ╭─[ functions_v2_duplicate_names.baml:3:3 ]
//    │
//  3 │   output string
//    │   ───┬──  
//    │      ╰──── Unknown variable `output`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `string`
//    ╭─[ functions_v2_duplicate_names.baml:3:10 ]
//    │
//  3 │   output string
//    │          ───┬──  
//    │             ╰──── Unknown variable `string`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:2:3 ]
//    │
//  2 │   input string
//    │   ──┬──  
//    │     ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:2:9 ]
//    │
//  2 │   input string
//    │         ───┬──  
//    │            ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ functions_v2_duplicate_names.baml:3:3 ]
//    │
//  3 │   output string
//    │   ───┬──  
//    │      ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Unknown variable `input`
//    ╭─[ functions_v2_duplicate_names.baml:2:3 ]
//    │
//  2 │   input string
//    │   ──┬──  
//    │     ╰──── Unknown variable `input`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `string`
//    ╭─[ functions_v2_duplicate_names.baml:2:9 ]
//    │
//  2 │   input string
//    │         ───┬──  
//    │            ╰──── Unknown variable `string`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `output`
//    ╭─[ functions_v2_duplicate_names.baml:3:3 ]
//    │
//  3 │   output string
//    │   ───┬──  
//    │      ╰──── Unknown variable `output`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `string`
//    ╭─[ functions_v2_duplicate_names.baml:3:10 ]
//    │
//  3 │   output string
//    │          ───┬──  
//    │             ╰──── Unknown variable `string`
//    │ 
//    │ Note: Error code: E0003
// ───╯
