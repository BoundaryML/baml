function simpleIf() -> string {
    let x = if true { "yes" } else { "no" }
    x
}

// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  test.baml:2
//    | 
//  1 | function simpleIf() -> string {
//  2 |     let x = if true { "yes" } else { "no" }
//  3 |     x
//    | 
// error: Error validating Function "simpleIf": This field declaration is invalid. It is either missing a name or a type.
//   -->  test.baml:3
//    | 
//  2 |     let x = if true { "yes" } else { "no" }
//  3 |     x
//    | 
// error: Invalid name for `function`: Must start with an uppercase letter.
//   -->  test.baml:1
//    | 
//    | 
//  1 | function simpleIf() -> string {
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  test.baml:2
//    | 
//  1 | function simpleIf() -> string {
//  2 |     let x = if true { "yes" } else { "no" }
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  test.baml:1
//    | 
//    | 
//  1 | function simpleIf() -> string {
//    | 
