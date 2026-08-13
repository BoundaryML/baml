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
// E0010
//
//   × invalid syntax
//    ╭─[headers_duplicate_function_calls.baml:1:1]
//  1 │ //# Pipeline
//    · ──────┬─────
//    ·       ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
// E0003
//
//   × unresolved name: `Foo`
//    ╭─[headers_duplicate_function_calls.baml:4:13]
//  4 │     let a = Foo();
//    ·             ───
//    ╰────
// E0003
//
//   × unresolved name: `Foo`
//    ╭─[headers_duplicate_function_calls.baml:7:13]
//  7 │     let b = Foo();
//    ·             ───
//    ╰────
// E0003
//
//   × unresolved name: `Foo`
//     ╭─[headers_duplicate_function_calls.baml:10:13]
//  10 │     let c = Foo();
//     ·             ───
//     ╰────
