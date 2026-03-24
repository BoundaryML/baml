enum Test {
    None
    Hello
    World
}

enum TestValidEnumKeyword {
    None @alias("None")
    Hello
    World
}

generator target{
    output_type "python/pydantic"
}

// error: Error validating field `None` in enum `Test`: Enum value 'None' is a reserved word in Python, try changing the name and using `OtherValueName @alias("None")`.
//   -->  enum/enum_is_valid.baml:2
//    |
//  1 | enum Test {
//  2 |     None
//  3 |     Hello
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
