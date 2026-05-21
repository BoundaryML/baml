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
// warning: unreachable arm
//    ╭─[ catch_catch_on_non_call.baml:4:10 ]
//    │
//  4 │     _ => "caught"
//    │          ────┬───  
//    │              ╰───── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// warning: unreachable arm
//     ╭─[ catch_catch_on_non_call.baml:10:10 ]
//     │
//  10 │     _ => "caught"
//     │          ────┬───  
//     │              ╰───── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
