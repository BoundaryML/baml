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
// error: type mismatch: expected int, got 10.0
//    ╭─[ expr_let_annotations.baml:5:17 ]
//    │
//  5 │   let y : int = 10.0;
//    │                 ──┬─  
//    │                   ╰─── type mismatch: expected int, got 10.0
//    │ 
//    │ Note: Error code: E0001
// ───╯
// error: type mismatch: expected int, got "hello"
//    ╭─[ expr_let_annotations.baml:6:16 ]
//    │
//  6 │   let c: int = "hello";
//    │                ───┬───  
//    │                   ╰───── type mismatch: expected int, got "hello"
//    │ 
//    │ Note: Error code: E0001
// ───╯
