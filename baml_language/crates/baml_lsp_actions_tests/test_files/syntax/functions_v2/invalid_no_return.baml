function FooBar(arg: int) {
  client "openai/gpt-3.5-turbo"
  prompt #""#
}

// error: Invalid syntax for function "FooBar". Use:
// function FooBar(params...) -> ReturnType { ... }
//   -->  functions_v2/invalid_no_return.baml:1
//    |
//    |
//  1 | function FooBar(arg: int) {
//    |

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
// Error: Expected return type (->), found '{'
//    ╭─[ functions_v2_invalid_no_return.baml:1:27 ]
//    │
//  1 │ function FooBar(arg: int) {
//    │                           ┬  
//    │                           ╰── Expected return type (->), found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
