// throws never with escaping throws — error

function ThrowsNeverViolation(x: int) -> string throws never {
  if (x < 0) {
    throw "escaped"
  }
  "ok"
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
// Error: throws contract violation: `never` is missing string
//    ╭─[ throw_throws_never_violation.baml:3:57 ]
//    │
//  3 │ function ThrowsNeverViolation(x: int) -> string throws never {
//    │                                                         ───┬──  
//    │                                                            ╰──── throws contract violation: `never` is missing string
//    │ 
//    │ Note: Error code: E0001
// ───╯
