function MayFail(x: int) -> string {
  "ok"
}

function InvalidArmSyntax(x: int) -> string {
  MayFail(x) catch (e) {
    _: string => "ok",
    let y = "hi";
  }
}

//----
//- diagnostics
// Error: Expected '=>' after catch pattern, found '='
//    ╭─[ catch_invalid_arm_syntax.baml:8:11 ]
//    │
//  8 │     let y = "hi";
//    │           ┬  
//    │           ╰── Expected '=>' after catch pattern, found '='
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Warning: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:18 ]
//    │
//  7 │     _: string => "ok",
//    │                  ──┬─  
//    │                    ╰─── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// Warning: unreachable arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:23 ]
//    │
//  7 │ ╭─▶     _: string => "ok",
//  8 │ ├─▶     let y = "hi";
//    │ │                       
//    │ ╰─────────────────────── unreachable arm
//    │     
//    │     Note: Error code: E0063
// ───╯
