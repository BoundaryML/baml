//# Loop Processing
function ForLoopWithHeaders() -> int {
    let items = [1, 2, 3, 4, 5];
    let result = 0;

    //## Main Loop
    for (let item in items) {
        //### Item Processing
        let processed = item * 2;

        //### Accumulation
        let result2 = result + processed;

        result2
    }

    //## Final Result
    result
}

//----
//- diagnostics
// Error: Statement must end with a semicolon.
//     ╭─[ headers_for_loop_mermaid.baml:7:5 ]
//     │
//   7 │ ╭─▶     for (let item in items) {
//     ┆ ┆   
//  15 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
