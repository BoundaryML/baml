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
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_on_non_call.baml:10:5 ]
//     │
//  10 │     _ => "caught"
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//    ╭─[ catch_catch_on_non_call.baml:4:5 ]
//    │
//  4 │     _ => "caught"
//    │     ┬  
//    │     ╰── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
