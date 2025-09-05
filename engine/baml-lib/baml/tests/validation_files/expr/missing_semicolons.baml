// Missing semicolons.
function Foo(x: int) -> int {
    let y = "hello"
    x
}

let x = 1

// error: Statement must end with a semicolon.
//   -->  expr/missing_semicolons.baml:3
//    | 
//  2 | function Foo(x: int) -> int {
//  3 |     let y = "hello"
//  4 |     x
//    | 
// error: Statement must end with a semicolon.
//   -->  expr/missing_semicolons.baml:7
//    | 
//  6 | 
//  7 | let x = 1
//  8 | 
//    | 
