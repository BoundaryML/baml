// If-let with no else used in value position must emit `VoidUsedAsValue`,
// mirroring plain `if` without else. (Caught by CodeRabbit on #3579.)

function NoElseInValuePosition(x: int | string) -> string {
  if let v: int = x {
    "matched"
  }
}

//----
//- diagnostics
// error: `if` without `else` cannot be used as a value; add an `else` branch
//    ╭─[ if_let_if_let_no_else_in_value_position.baml:4:60 ]
//    │
//  4 │ ╭─▶ function NoElseInValuePosition(x: int | string) -> string {
//    ┆ ┆   
//  7 │ ├─▶   }
//    │ │         
//    │ ╰───────── `if` without `else` cannot be used as a value; add an `else` branch
//    │     
//    │     Note: Error code: E0001
// ───╯
