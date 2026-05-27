// The else block must diverge (`!`). Returning a value is an error (E0113).
class Ok { value string }
class Err { message string }

function NotDiverging(r: Ok | Err) -> string {
  let o: Ok = r else { "fallback" };
  o.value
}

//----
//- diagnostics
// error: `let … else` requires a diverging else block (`return`, `throw`, `break`, `continue`, or an infinite loop); got `"fallback"`
//    ╭─[ let_else_let_else_must_diverge.baml:6:21 ]
//    │
//  6 │   let o: Ok = r else { "fallback" };
//    │                     ───────┬───────  
//    │                            ╰───────── `let … else` requires a diverging else block (`return`, `throw`, `break`, `continue`, or an infinite loop); got `"fallback"`
//    │ 
//    │ Note: Error code: E0113
// ───╯
