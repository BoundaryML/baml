function MayFail(x: int) -> string {
  "ok"
}

function InvalidArmSyntax(x: int) -> string {
  MayFail(x) catch (e) {
    string => "ok",
    let y = "hi";
  }
}

//----
//- diagnostics
// error: Expected '=>' after catch pattern, found '='
//    ╭─[ catch_invalid_arm_syntax.baml:8:11 ]
//    │
//  8 │     let y = "hi";
//    │           ┬  
//    │           ╰── Expected '=>' after catch pattern, found '='
//    │ 
//    │ Note: Error code: E0010
// ───╯
// warning: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:15 ]
//    │
//  7 │     string => "ok",
//    │               ──┬─  
//    │                 ╰─── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// warning: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:20 ]
//    │
//  7 │ ╭─▶     string => "ok",
//  8 │ ├─▶     let y = "hi";
//    │ │                       
//    │ ╰─────────────────────── unreachable arm
//    │     
//    │     Note: Error code: E0063
// ───╯
