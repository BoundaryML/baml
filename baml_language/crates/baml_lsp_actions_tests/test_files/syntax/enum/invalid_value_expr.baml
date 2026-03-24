enum Test {
    A int | string
}

// error: Error validating: Unexpected type specified for value `A`
//   -->  enum/invalid_value_expr.baml:2
//    |
//  1 | enum Test {
//  2 |     A int | string
//  3 | }
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
// Error: Enum variants cannot have type annotations
//    ╭─[ enum_invalid_value_expr.baml:2:7 ]
//    │
//  2 │     A int | string
//    │       ──────┬─────  
//    │             ╰─────── Enum variants cannot have type annotations
//    │ 
//    │ Note: Error code: E0010
// ───╯
