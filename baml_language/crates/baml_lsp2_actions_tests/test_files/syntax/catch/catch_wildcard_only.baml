// catch with only wildcard arm — unreachable because MayFail never throws
function MayFail(x: int) -> string {
  "ok"
}

function CatchWildcardOnly(x: int) -> string {
  MayFail(x) catch (e) {
    _ => "caught everything"
  }
}

// Wildcard with binding name
function CatchNamedWildcard(x: int) -> string {
  MayFail(x) catch (e) {
    let other => "caught: " + other
  }
}

//----
//- diagnostics
// warning: unreachable arm
//    ╭─[ catch_catch_wildcard_only.baml:8:10 ]
//    │
//  8 │     _ => "caught everything"
//    │          ─────────┬─────────  
//    │                   ╰─────────── unreachable arm
//    │ 
//    │ Note: Error code: E0063
// ───╯
// warning: unreachable arm
//     ╭─[ catch_catch_wildcard_only.baml:15:17 ]
//     │
//  15 │     let other => "caught: " + other
//     │                 ─────────┬─────────  
//     │                          ╰─────────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
