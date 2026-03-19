function ForLoopWithHeaders() -> int {
    //# Loop Processing
    let items = [1, 2, 3, 4, 5];
    let result = 0;

    //## Main Loop
    for (let item in items) {
        //### Item Processing
        let processed = item * 2;

        //#### Accumulation
        let result2 = result + processed;

        result2
    }

    //## Final Result
    result
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_for_loops.baml:1
//    |
//    |
//  1 | function ForLoopWithHeaders() -> int {
//    |

//----
//- diagnostics
// Error: unresolved name: item
//    ╭─[ headers_for_loops.baml:9:25 ]
//    │
//  9 │         let processed = item * 2;
//    │                         ──┬─  
//    │                           ╰─── unresolved name: item
//    │ 
//    │ Note: Error code: E0001
// ───╯
