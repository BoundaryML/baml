//# Pipeline
function CallsDedup() -> string {
    //## Step A
    let a = Foo();

    //## Step B
    let b = Foo();

    //## Step C
    let c = Foo();

    a + b
}

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
// Error: unresolved name: Foo
//    ╭─[ headers_duplicate_function_calls.baml:4:13 ]
//    │
//  4 │     let a = Foo();
//    │             ─┬─  
//    │              ╰─── unresolved name: Foo
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unresolved name: Foo
//    ╭─[ headers_duplicate_function_calls.baml:7:13 ]
//    │
//  7 │     let b = Foo();
//    │             ─┬─  
//    │              ╰─── unresolved name: Foo
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unresolved name: Foo
//     ╭─[ headers_duplicate_function_calls.baml:10:13 ]
//     │
//  10 │     let c = Foo();
//     │             ─┬─  
//     │              ╰─── unresolved name: Foo
//     │ 
//     │ Note: Error code: E0001
// ────╯
