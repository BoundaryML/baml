class InterfaceThree {
  interface int
  foo
}

// error: Error validating: No type specified for field `foo`
//   -->  class/incomplete_class.baml:3
//    |
//  2 |   interface int
//  3 |   foo
//  4 | }
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
// Error: field 'foo' is missing a type annotation
//    ╭─[ class_incomplete_class.baml:3:3 ]
//    │
//  3 │   foo
//    │   ─┬─  
//    │    ╰─── field 'foo' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
