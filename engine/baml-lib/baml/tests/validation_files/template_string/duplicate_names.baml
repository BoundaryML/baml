template_string SomeString #"
  ...
"#

template_string SomeString(a: int) #"
  ...
"#

function SomeString {
  input string
  output string
}

// error: Error validating function "SomeString": Invalid function syntax. Valid function syntax is
// ```
// function SomeString(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  template_string/duplicate_names.baml:9
//    | 
//  8 | 
//  9 | function SomeString {
// 10 |   input string
// 11 |   output string
// 12 | }
//    | 
