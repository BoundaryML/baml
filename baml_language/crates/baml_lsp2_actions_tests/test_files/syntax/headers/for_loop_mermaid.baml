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
// E0010
//
//   × invalid syntax
//    ╭─[headers_for_loop_mermaid.baml:1:1]
//  1 │ //# Loop Processing
//    · ─────────┬─────────
//    ·          ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
