function Bar {
  input string
  output string
}

function Bar(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

// error: Error validating function "Bar": Invalid syntax: missing input parameters and return type. Valid function syntax is
// ```
// function Bar(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/duplicate_names.baml:1
//    | 
//    | 
//  1 | function Bar {
//  2 |   input string
//  3 |   output string
//  4 | }
//    | 
