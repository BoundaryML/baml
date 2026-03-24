function NoRet(x: int) {
    1
}

// error: function must have a return type: e.g. function Foo() -> int
//   -->  expr/missing_return_value.baml:1
//    |
//    |
//  1 | function NoRet(x: int) {
//  2 |     1
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
// Error: Expected return type (->), found '{'
//    ╭─[ expr_missing_return_value.baml:1:24 ]
//    │
//  1 │ function NoRet(x: int) {
//    │                        ┬  
//    │                        ╰── Expected return type (->), found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
