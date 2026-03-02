// catch with only wildcard arm — no diagnostics expected
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
    other => "caught: " + other
  }
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//     ╭─[ catch_catch_wildcard_only.baml:15:7 ]
//     │
//  15 │     other => "caught: " + other
//     │       ──┬──  
//     │         ╰──── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Warning: Unreachable catch arm
//    ╭─[ catch_catch_wildcard_only.baml:8:7 ]
//    │
//  8 │     _ => "caught everything"
//    │       ┬  
//    │       ╰── Unreachable catch arm
//    │ 
//    │ Note: Error code: E0095
// ───╯
