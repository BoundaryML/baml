class TestLiterals {
  a "SingleLiteral"
  b 2 | 3.25
}

// error: Error validating: Float literal values are not supported: 3.25
//   -->  class/unsupported_literal_types.baml:3
//    |
//  2 |   a "SingleLiteral"
//  3 |   b 2 | 3.25
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
// Error: Float literal values are not supported: 3.25
//    ╭─[ class_unsupported_literal_types.baml:3:9 ]
//    │
//  3 │   b 2 | 3.25
//    │         ──┬─  
//    │           ╰─── Float literal values are not supported: 3.25
//    │ 
//    │ Note: Error code: E0010
// ───╯
