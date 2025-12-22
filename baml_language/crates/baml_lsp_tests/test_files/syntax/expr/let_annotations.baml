let G: int | float = 10.0;

function Ok() -> int {
  let x: int | float = 10.0;
  let y : int = 10.0;
  let c: int = "hello";
  1
}

// error: Error validating: Type mismatch: variable 'y' annotated as int but got float
//   -->  expr/let_annotations.baml:5
//    | 
//  4 |   let x: int | float = 10.0;
//  5 |   let y : int = 10.0;
//  6 |   let c: int = "hello";
//    | 
// error: Error validating: Type mismatch: variable 'c' annotated as int but got string
//   -->  expr/let_annotations.baml:6
//    | 
//  5 |   let y : int = 10.0;
//  6 |   let c: int = "hello";
//  7 |   1
//    |

//----
//- diagnostics
// Error: Expected top-level declaration, found Let
//    ╭─[ 0:1:1 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │ ─┬─  
//    │  ╰─── Expected top-level declaration, found Let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:5 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │     ┬  
//    │     ╰── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Colon
//    ╭─[ 0:1:6 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │      ┬  
//    │      ╰── Expected top-level declaration, found Colon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:8 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │        ─┬─  
//    │         ╰─── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Pipe
//    ╭─[ 0:1:12 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │            ┬  
//    │            ╰── Expected top-level declaration, found Pipe
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//    ╭─[ 0:1:14 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │              ──┬──  
//    │                ╰──── Expected top-level declaration, found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Equals
//    ╭─[ 0:1:20 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │                    ┬  
//    │                    ╰── Expected top-level declaration, found Equals
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found FloatLiteral
//    ╭─[ 0:1:22 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │                      ──┬─  
//    │                        ╰─── Expected top-level declaration, found FloatLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Semicolon
//    ╭─[ 0:1:26 ]
//    │
//  1 │ let G: int | float = 10.0;
//    │                          ┬  
//    │                          ╰── Expected top-level declaration, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected int | float, found float
//    ╭─[ 0:4:9 ]
//    │
//  4 │   let x: int | float = 10.0;
//    │         ──────┬─────  
//    │               ╰─────── Expected int | float, found float
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected int, found float
//    ╭─[ 0:5:10 ]
//    │
//  5 │   let y : int = 10.0;
//    │          ──┬─  
//    │            ╰─── Expected int, found float
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected int, found string
//    ╭─[ 0:6:9 ]
//    │
//  6 │   let c: int = "hello";
//    │         ──┬─  
//    │           ╰─── Expected int, found string
//    │ 
//    │ Note: Error code: E0001
// ───╯
