class InterfaceTwo {
  interface string
  prop2 Bar
}

// error: Type `Bar` does not exist. Did you mean one of these: `int`, `bool`, `float`, `true`, `false`, `string`, `InterfaceTwo`?
//   -->  class/unknown_type.baml:3
//    |
//  2 |   interface string
//  3 |   prop2 Bar
//    |

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
// Error: unresolved type: Bar
//    ╭─[ class_unknown_type.baml:3:8 ]
//    │
//  3 │   prop2 Bar
//    │        ──┬─  
//    │          ╰─── unresolved type: Bar
//    │ 
//    │ Note: Error code: E0002
// ───╯
