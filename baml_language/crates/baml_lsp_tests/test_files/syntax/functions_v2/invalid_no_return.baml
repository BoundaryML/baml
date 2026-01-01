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
// Error: Expected return type (->), found LBrace
//    ╭─[ 0:1:27 ]
//    │
//  1 │ function FooBar(arg: int) {
//    │                           ┬  
//    │                           ╰── Expected return type (->), found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
