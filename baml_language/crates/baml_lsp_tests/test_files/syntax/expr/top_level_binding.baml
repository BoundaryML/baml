let x = 1;

let y = {
    let b = 2;
    [1,2,3]
};

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:1:1 ]
//    │
//  1 │ let x = 1;
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:5 ]
//    │
//  1 │ let x = 1;
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:1:7 ]
//    │
//  1 │ let x = 1;
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:1:9 ]
//    │
//  1 │ let x = 1;
//    │         ┬  
//    │         ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:1:10 ]
//    │
//  1 │ let x = 1;
//    │          ┬  
//    │          ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:3:1 ]
//    │
//  3 │ let y = {
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:3:5 ]
//    │
//  3 │ let y = {
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:3:7 ]
//    │
//  3 │ let y = {
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBrace
//    ╭─[ 0:3:9 ]
//    │
//  3 │ let y = {
//    │         ┬  
//    │         ╰── Expected top-level declaration, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:4:5 ]
//    │
//  4 │     let b = 2;
//    │     ─┬─  
//    │      ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:4:9 ]
//    │
//  4 │     let b = 2;
//    │         ┬  
//    │         ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:4:11 ]
//    │
//  4 │     let b = 2;
//    │           ┬  
//    │           ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:4:13 ]
//    │
//  4 │     let b = 2;
//    │             ┬  
//    │             ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:4:14 ]
//    │
//  4 │     let b = 2;
//    │              ┬  
//    │              ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found LBracket
//    ╭─[ 0:5:5 ]
//    │
//  5 │     [1,2,3]
//    │     ┬  
//    │     ╰── Expected top-level declaration, found LBracket
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:5:6 ]
//    │
//  5 │     [1,2,3]
//    │      ┬  
//    │      ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:5:7 ]
//    │
//  5 │     [1,2,3]
//    │       ┬  
//    │       ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:5:8 ]
//    │
//  5 │     [1,2,3]
//    │        ┬  
//    │        ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Comma
//    ╭─[ 0:5:9 ]
//    │
//  5 │     [1,2,3]
//    │         ┬  
//    │         ╰── Expected top-level declaration, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found IntegerLiteral
//    ╭─[ 0:5:10 ]
//    │
//  5 │     [1,2,3]
//    │          ┬  
//    │          ╰── Expected top-level declaration, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBracket
//    ╭─[ 0:5:11 ]
//    │
//  5 │     [1,2,3]
//    │           ┬  
//    │           ╰── Expected top-level declaration, found RBracket
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:6:1 ]
//    │
//  6 │ };
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:6:2 ]
//    │
//  6 │ };
//    │  ┬  
//    │  ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
