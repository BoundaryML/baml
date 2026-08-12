
//# Main Function
function ComplexHeaderTest() -> string {
    //# Initialization
    let items = ["a", "b", "c"];
    let result = "";

    //## Processing Loop
    for (let item in items) {

        //# Markdown Header tester
        let hello = "Hello";

        //### Item Analysis
        let processed = 2;

        //### Validation
        let z = if (true) {

            //# Markdown Header tester in statement
            let hello = "Hello";

            //### another header in the statement
            let hello = "Hello";

            //### True totally
            12
        } else {
            //### False other branch
            13
        };

        //## very useful header
        let hello2 = "Hello";

        //### Result
        result
    }

    //# Final Processing
    let final_result = 213;

    //# Return Result
    final_result
}

//# Loop Processing
function ForLoopWithHeaders() -> int {
    let items = [1, 2, 3, 4, 5];
    let result = 0;

    //## Main Loop
    for (let item in items) {
        //### Item Processing
        let processed = item * 2;

        //### Accumulation
        let result = result + processed;

        processed
    }

    //## Final Result
    result
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[headers_complex_headers_test.baml:2:40]
//   2 │ ╭─▶ function ComplexHeaderTest() -> string {
//   3 │ │       //# Initialization
//   4 │ │       let items = ["a", "b", "c"];
//   5 │ │       let result = "";
//   6 │ │
//   7 │ │       //## Processing Loop
//   8 │ │       for (let item in items) {
//   9 │ │
//  10 │ │           //# Markdown Header tester
//  11 │ │           let hello = "Hello";
//  12 │ │
//  13 │ │           //### Item Analysis
//  14 │ │           let processed = 2;
//  15 │ │
//  16 │ │           //### Validation
//  17 │ │           let z = if (true) {
//  18 │ │
//  19 │ │               //# Markdown Header tester in statement
//  20 │ │               let hello = "Hello";
//  21 │ │
//  22 │ │               //### another header in the statement
//  23 │ │               let hello = "Hello";
//  24 │ │
//  25 │ │               //### True totally
//  26 │ │               12
//  27 │ │           } else {
//  28 │ │               //### False other branch
//  29 │ │               13
//  30 │ │           };
//  31 │ │
//  32 │ │           //## very useful header
//  33 │ │           let hello2 = "Hello";
//  34 │ │
//  35 │ │           //### Result
//  36 │ │           result
//  37 │ │       }
//  38 │ │
//  39 │ │       //# Final Processing
//  40 │ │       let final_result = 213;
//  41 │ │
//  42 │ │       //# Return Result
//  43 │ │       final_result
//  44 │ ├─▶ }
//     · ╰──── expected `string`, found `int`
//     ╰────
