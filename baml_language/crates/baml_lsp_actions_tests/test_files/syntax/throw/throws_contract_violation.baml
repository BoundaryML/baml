// throws contract violation: inferred exceeds declared — error

function ThrowsViolation(x: int) -> string throws string {
  match (x) {
    0 => throw "error",
    1 => throw 42,
    _ => "ok"
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
// Error: throws contract violation: `string` is missing int
//    ╭─[ throw_throws_contract_violation.baml:3:52 ]
//    │
//  3 │ function ThrowsViolation(x: int) -> string throws string {
//    │                                                    ───┬───  
//    │                                                       ╰───── throws contract violation: `string` is missing int
//    │ 
//    │ Note: Error code: E0001
// ───╯
