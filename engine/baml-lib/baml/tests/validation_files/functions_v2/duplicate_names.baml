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
