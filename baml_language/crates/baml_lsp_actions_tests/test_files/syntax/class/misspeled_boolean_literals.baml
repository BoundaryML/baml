class TestLiterals {
  a "SingleLiteral"
  b "boolean" | True | False
}

// error: Type `True` does not exist. Did you mean one of these: `true`, `string`, `int`, `bool`, `false`, `float`, `TestLiterals`?
//   -->  class/misspeled_boolean_literals.baml:3
//    |
//  2 |   a "SingleLiteral"
//  3 |   b "boolean" | True | False
//    |
// error: Type `False` does not exist. Did you mean one of these: `false`, `float`, `true`, `int`, `bool`, `string`, `TestLiterals`?
//   -->  class/misspeled_boolean_literals.baml:3
//    |
//  2 |   a "SingleLiteral"
//  3 |   b "boolean" | True | False
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
// Error: unresolved type: False
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: False
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: True
//    ╭─[ class_misspeled_boolean_literals.baml:3:4 ]
//    │
//  3 │   b "boolean" | True | False
//    │    ────────────┬────────────  
//    │                ╰────────────── unresolved type: True
//    │ 
//    │ Note: Error code: E0002
// ───╯
