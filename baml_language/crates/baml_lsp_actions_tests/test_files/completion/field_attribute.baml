class Person {
    name @<[CURSOR] string
}

//----
//- diagnostics
// (parse error expected - incomplete attribute)
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
// Error: field 'name' is missing a type annotation
//    ╭─[ field_attribute.baml:2:5 ]
//    │
//  2 │     name @ string
//    │     ──┬─  
//    │       ╰─── field 'name' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in class body, found '@'
//    ╭─[ field_attribute.baml:2:10 ]
//    │
//  2 │     name @ string
//    │          ┬  
//    │          ╰── Expected Unexpected token in class body, found '@'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: field 'string' is missing a type annotation
//    ╭─[ field_attribute.baml:2:12 ]
//    │
//  2 │     name @ string
//    │            ───┬──  
//    │               ╰──── field 'string' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
