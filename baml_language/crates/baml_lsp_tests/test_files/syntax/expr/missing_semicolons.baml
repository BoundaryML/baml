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

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:7:1 ]
//    │
//  7 │ let x = 1
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:7:5 ]
//    │
//  7 │ let x = 1
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:7:7 ]
//    │
//  7 │ let x = 1
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:7:9 ]
//    │
//  7 │ let x = 1
//    │         ┬  
//    │         ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
