// catch attached to a non-callable expression should error
function CatchOnLiteral(x: int) -> string {
  "hello" catch (e) {
    _ => "caught"
  }
}

function CatchOnVariable(x: int) -> string {
  x catch (e) {
    _ => "caught"
  }
}

//----
//- diagnostics
// Error: catch clauses can only be attached to callable expressions
//    ╭─[ catch_catch_on_non_call.baml:3:11 ]
//    │
//  3 │   "hello" catch (e) {
//    │           ──┬──  
//    │             ╰──── catch clauses can only be attached to callable expressions
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: catch clauses can only be attached to callable expressions
//    ╭─[ catch_catch_on_non_call.baml:9:5 ]
//    │
//  9 │   x catch (e) {
//    │     ──┬──  
//    │       ╰──── catch clauses can only be attached to callable expressions
//    │ 
//    │ Note: Error code: E0010
// ───╯
