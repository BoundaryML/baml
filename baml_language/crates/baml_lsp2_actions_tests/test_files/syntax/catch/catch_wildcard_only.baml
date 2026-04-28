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
// Warning: unreachable arm
//    ╭─[ catch_catch_wildcard_only.baml:8:12 ]
//    │
//  8 │ ╭─▶     _ => "caught everything"
//  9 │ ├─▶   }
//    │ │         
//    │ ╰───────── unreachable arm
//    │     
//    │     Note: Error code: E0063
// ───╯
// Warning: unreachable arm
//     ╭─[ catch_catch_wildcard_only.baml:15:19 ]
//     │
//  15 │ ╭─▶     let other => "caught: " + other
//  16 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0063
// ────╯
