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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//    ╭─[ catch_catch_wildcard_only.baml:8:12 ]
//    │
//  8 │ ╭─▶     _ => "caught everything"
//  9 │ ├─▶   }
//    │ │         
//    │ ╰───────── unreachable arm
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: unreachable arm
//     ╭─[ catch_catch_wildcard_only.baml:15:15 ]
//     │
//  15 │ ╭─▶     other => "caught: " + other
//  16 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
