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
// E0010
//
//   × unexpected token
//    ╭─[let_else_let_else_no_else_if.baml:7:22]
//  7 │   let o: Ok = r else if true { return "x"; };
//    ·                      ─┬
//    ·                       ╰── expected `block after 'else' (`else if` is not valid in let-else)`, found `if`
//    ╰────
