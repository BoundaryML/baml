//# Main Function
function NestedIfStatements() -> string {
    //## If statement 1
    let e = if (true) {

        //### If statement 2.1
        let z = if (true) {
            //### If statement 3.1
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };

            242
        } else {
            //### If statement 3.2
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };
            13
        };

        2
    } else {
        //### If statement 2.2
        let z = if (true) {
            //### If statement 3.1
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };

            242
        } else {
            //### If statement 3.2
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };
            13
        };
        13
    };

    e
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[headers_nested_if_statements.baml:2:41]
//   2 │ ╭─▶ function NestedIfStatements() -> string {
//   3 │ │       //## If statement 1
//   4 │ │       let e = if (true) {
//   5 │ │
//   6 │ │           //### If statement 2.1
//   7 │ │           let z = if (true) {
//   8 │ │               //### If statement 3.1
//   9 │ │               let a = if (true) {
//  10 │ │                   //### True
//  11 │ │                   12
//  12 │ │               } else {
//  13 │ │                   //### Third False
//  14 │ │                   13
//  15 │ │               };
//  16 │ │
//  17 │ │               242
//  18 │ │           } else {
//  19 │ │               //### If statement 3.2
//  20 │ │               let a = if (true) {
//  21 │ │                   //### True
//  22 │ │                   12
//  23 │ │               } else {
//  24 │ │                   //### Third False
//  25 │ │                   13
//  26 │ │               };
//  27 │ │               13
//  28 │ │           };
//  29 │ │
//  30 │ │           2
//  31 │ │       } else {
//  32 │ │           //### If statement 2.2
//  33 │ │           let z = if (true) {
//  34 │ │               //### If statement 3.1
//  35 │ │               let a = if (true) {
//  36 │ │                   //### True
//  37 │ │                   12
//  38 │ │               } else {
//  39 │ │                   //### Third False
//  40 │ │                   13
//  41 │ │               };
//  42 │ │
//  43 │ │               242
//  44 │ │           } else {
//  45 │ │               //### If statement 3.2
//  46 │ │               let a = if (true) {
//  47 │ │                   //### True
//  48 │ │                   12
//  49 │ │               } else {
//  50 │ │                   //### Third False
//  51 │ │                   13
//  52 │ │               };
//  53 │ │               13
//  54 │ │           };
//  55 │ │           13
//  56 │ │       };
//  57 │ │
//  58 │ │       e
//  59 │ ├─▶ }
//     · ╰──── expected `string`, found `int`
//     ╰────
