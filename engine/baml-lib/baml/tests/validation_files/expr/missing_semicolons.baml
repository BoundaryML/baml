// Missing semicolons.
function Foo(x: int) -> int {
    let y = "hello"
    x
}

let x = 1

// error: Error validating Function "Foo": This field declaration is invalid. It is either missing a name or a type.
//   -->  expr/missing_semicolons.baml:4
//    | 
//  3 |     let y = "hello"
//  4 |     x
//    | 
// error: Error validating: Unexpected keyword used in assignment: let
//   -->  expr/missing_semicolons.baml:7
//    | 
//  6 | 
//  7 | let x = 1
//    | 
// error: Invalid name for `type alias`: Must start with an uppercase letter.
//   -->  expr/missing_semicolons.baml:7
//    | 
//  6 | 
//  7 | let x = 1
//    | 
// error: Error validating: Unknown field `let` in function
//   -->  expr/missing_semicolons.baml:3
//    | 
//  2 | function Foo(x: int) -> int {
//  3 |     let y = "hello"
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  expr/missing_semicolons.baml:2
//    | 
//  1 | // Missing semicolons.
//  2 | function Foo(x: int) -> int {
//    | 
