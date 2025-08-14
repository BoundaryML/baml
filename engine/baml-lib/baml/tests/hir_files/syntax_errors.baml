function Test() -> int {
  let x = 1
  x
}

// error: error: Error validating Function "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  test.baml:3
//    | 
//  2 |   let x = 1
//  3 |   x
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:2
//    | 
//  1 | function Test() -> int {
//  2 |   let x = 1
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  test.baml:1
//    | 
//    | 
//  1 | function Test() -> int {
//    | 
//
