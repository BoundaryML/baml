function Foo() -> int {
    let x = {
        let y = 1
        y
    }
    x
}

// error: error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  test.baml:2
//    | 
//  1 | function Foo() -> int {
//  2 |     let x = {
//  3 |         let y = 1
//    | 
// error: Error validating Function "Foo": This field declaration is invalid. It is either missing a name or a type.
//   -->  test.baml:4
//    | 
//  3 |         let y = 1
//  4 |         y
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:6
//    | 
//  5 |     }
//  6 |     x
//  7 | }
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:2
//    | 
//  1 | function Foo() -> int {
//  2 |     let x = {
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:3
//    | 
//  2 |     let x = {
//  3 |         let y = 1
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  test.baml:1
//    | 
//    | 
//  1 | function Foo() -> int {
//    | 