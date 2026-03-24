enum Test {
    ONE
    TWO
    ONE
}

// error: Value "ONE" is already defined on enum "Test".
//   -->  enum/duplicate_value.baml:4
//    |
//  3 |     TWO
//  4 |     ONE
//  5 | }
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
// Error: Duplicate variant `Test.ONE`
//    ╭─[ enum_duplicate_value.baml:4:5 ]
//    │
//  2 │     ONE
//    │     ─┬─  
//    │      ╰─── first defined as variant here
//    │ 
//  4 │     ONE
//    │     ─┬─  
//    │      ╰─── duplicate variant definition
//    │ 
//    │ Note: Error code: E0012
// ───╯
