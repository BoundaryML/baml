class A {
  field string
}

class A {
  field string
}

type A = int

// error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".
//   -->  class/duplicate_definitions.baml:1
//    |
//    |
//  1 | class A {
//    |
//   -->  class/duplicate_definitions.baml:5
//    |
//  4 |
//  5 | class A {
//    |
//   -->  class/duplicate_definitions.baml:9
//    |
//  8 |
//  9 | type A = int
//    |
// error: The class "A" cannot be re-defined because it is already defined as one of these: "class", "type_alias".
//   -->  class/duplicate_definitions.baml:5
//    |
//  4 |
//  5 | class A {
//    |
//   -->  class/duplicate_definitions.baml:1
//    |
//    |
//  1 | class A {
//    |
//   -->  class/duplicate_definitions.baml:9
//    |
//  8 |
//  9 | type A = int
//    |
// error: The type_alias "A" cannot be re-defined because it is already defined as a class.
//   -->  class/duplicate_definitions.baml:9
//    |
//  8 |
//  9 | type A = int
//    |
//   -->  class/duplicate_definitions.baml:1
//    |
//    |
//  1 | class A {
//    |
//   -->  class/duplicate_definitions.baml:5
//    |
//  4 |
//  5 | class A {
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
