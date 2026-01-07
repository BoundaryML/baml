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
// Error: Unknown variable `Foo`
//    ╭─[ headers_duplicate_function_calls.baml:4:13 ]
//    │
//  4 │     let a = Foo();
//    │             ─┬─  
//    │              ╰─── Unknown variable `Foo`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `Foo`
//    ╭─[ headers_duplicate_function_calls.baml:7:13 ]
//    │
//  7 │     let b = Foo();
//    │             ─┬─  
//    │              ╰─── Unknown variable `Foo`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `Foo`
//     ╭─[ headers_duplicate_function_calls.baml:10:13 ]
//     │
//  10 │     let c = Foo();
//     │             ─┬─  
//     │              ╰─── Unknown variable `Foo`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Cannot apply operator 'Add' to types `unknown` and `unknown`
//     ╭─[ headers_duplicate_function_calls.baml:12:5 ]
//     │
//  12 │     a + b
//     │     ──┬──  
//     │       ╰──── Cannot apply operator 'Add' to types `unknown` and `unknown`
//     │ 
//     │ Note: Error code: E0004
// ────╯
