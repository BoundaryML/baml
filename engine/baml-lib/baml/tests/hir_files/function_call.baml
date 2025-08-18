function myFunc(x: int, y: string) -> int {
    x
}

function CallTest() -> int {
    let result = myFunc(42, "hello")
    result
}

// error: error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  test.baml:6
//    | 
//  5 | function CallTest() -> int {
//  6 |     let result = myFunc(42, "hello")
//  7 |     result
//    | 
// error: Error validating Function "CallTest": This field declaration is invalid. It is either missing a name or a type.
//   -->  test.baml:7
//    | 
//  6 |     let result = myFunc(42, "hello")
//  7 |     result
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:6
//    | 
//  5 | function CallTest() -> int {
//  6 |     let result = myFunc(42, "hello")
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  test.baml:5
//    | 
//  4 | 
//  5 | function CallTest() -> int {
//    | 
//
