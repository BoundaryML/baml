generator default {
  language python
  o o
}

// error: Error validating function "default": Invalid syntax: missing input parameters and return type. Valid function syntax is
// ```
// function default(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/error.baml:1
//    | 
//    | 
//  1 | generator default {
//  2 |   language python
//  3 |   o o
//  4 | }
//    | 
