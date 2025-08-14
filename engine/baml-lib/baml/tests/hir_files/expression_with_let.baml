function AddOne(x: int) -> int {
    let y = x
    y
}

// error: error: Error validating Function "AddOne": This field declaration is invalid. It is either missing a name or a type.
/
//    | 
//  2 |     let y = x
//  3 |     y
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:2
//    | 
//  1 | function AddOne(x: int) -> int {
//  2 |     let y = x
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  test.baml:1
//    | 
//    | 
//  1 | function AddOne(x: int) -> int {
//    | 
//
