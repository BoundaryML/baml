// Pattern bindings are not visible inside the else branch — the else runs
// when the pattern did NOT match, so binding `o` doesn't exist there.
class Ok { value string }
class Err { message string }

function ElseDoesNotSeeBinding(r: Ok | Err) -> string {
  let o: Ok = r else { return o.value; };
  o.value
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `o.value`
//    ╭─[let_else_let_else_else_no_bindings.baml:7:31]
//  7 │   let o: Ok = r else { return o.value; };
//    ·                               ───────
//    ╰────
