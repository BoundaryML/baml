// `else if` after `let … else` is rejected at parse time. Unlike `if let`,
// there is no value being produced for the chain to feed into.
class Ok { value string }
class Err { message string }

function NoElseIfChain(r: Ok | Err) -> string {
  let o: Ok = r else if true { return "x"; };
  o.value
}

//----
//- diagnostics
// error: Expected block after 'else' (`else if` is not valid in let-else), found if
//    ╭─[ let_else_let_else_no_else_if.baml:7:22 ]
//    │
//  7 │   let o: Ok = r else if true { return "x"; };
//    │                      ─┬  
//    │                       ╰── Expected block after 'else' (`else if` is not valid in let-else), found if
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: type mismatch: expected Ok, got Ok | Err
//    ╭─[ let_else_let_else_no_else_if.baml:7:15 ]
//    │
//  7 │   let o: Ok = r else if true { return "x"; };
//    │               ┬  
//    │               ╰── type mismatch: expected Ok, got Ok | Err
//    │ 
//    │ Note: Error code: E0001
// ───╯
