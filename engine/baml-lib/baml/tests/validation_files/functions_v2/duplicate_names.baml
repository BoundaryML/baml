function Bar {
  input string
  output string
}

function Bar(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

// error: The function "Bar" cannot be defined because a function with that name already exists.
//   -->  functions_v2/duplicate_names.baml:6
//    | 
//  5 | 
//  6 | function Bar(a: string, b: int | bool) -> int {
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
