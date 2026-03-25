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
// Error: operator `Mul` cannot be applied to `1 | 2 | 3 | 4 | 5` and `2`
//    ╭─[ headers_for_loop_mermaid.baml:9:25 ]
//    │
//  9 │         let processed = item * 2;
//    │                         ────┬───  
//    │                             ╰───── operator `Mul` cannot be applied to `1 | 2 | 3 | 4 | 5` and `2`
//    │ 
//    │ Note: Error code: E0001
// ───╯
