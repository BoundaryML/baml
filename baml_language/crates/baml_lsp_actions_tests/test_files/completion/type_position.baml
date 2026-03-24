class MyClass {
    value string
}

enum MyEnum {
    A
    B
}

class Test {
    field :<[CURSOR]
}

//----
//- diagnostics
// (parse error expected - incomplete field)
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
// Error: field 'field' is missing a type annotation
//     ╭─[ type_position.baml:11:5 ]
//     │
//  11 │     field :
//     │     ──┬──  
//     │       ╰──── field 'field' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
//
//- completions
// NOTE: MyClass and MyEnum excluded pending compiler2 LSP HIR completions reimplementation
// SHOULD_CONTAIN: int, float, string, bool, null, image, audio
