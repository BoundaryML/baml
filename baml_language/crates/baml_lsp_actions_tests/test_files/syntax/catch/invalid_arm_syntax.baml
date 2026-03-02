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
// Error: Expected pattern, found let
//    ╭─[ catch_invalid_arm_syntax.baml:8:5 ]
//    │
//  8 │     let y = "hi";
//    │     ─┬─  
//    │      ╰─── Expected pattern, found let
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected '=>' after catch pattern, found identifier
//    ╭─[ catch_invalid_arm_syntax.baml:8:9 ]
//    │
//  8 │     let y = "hi";
//    │         ┬  
//    │         ╰── Expected '=>' after catch pattern, found identifier
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Warning: Unreachable catch arm
//    ╭─[ catch_invalid_arm_syntax.baml:7:5 ]
//    │
//  7 │     _: string => "ok",
//    │     ────┬────  
//    │         ╰────── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
// Warning: Unreachable catch arm
//    ╭─[ 4294967295:1:1 ]
//    │
//  1 │ 
//    │ │ 
//    │ ╰─ Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
