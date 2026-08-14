
//# Basic test
function BasicIf() -> string {
    let x = 1;

    //# This is a named statement
    let z = if (true) {
        //# Then Statement
        12
    } else {
        //# Else Statement
        13
    };

    z
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[headers_basic_if.baml:2:30]
//   2 │ ╭─▶ function BasicIf() -> string {
//   3 │ │       let x = 1;
//   4 │ │
//   5 │ │       //# This is a named statement
//   6 │ │       let z = if (true) {
//   7 │ │           //# Then Statement
//   8 │ │           12
//   9 │ │       } else {
//  10 │ │           //# Else Statement
//  11 │ │           13
//  12 │ │       };
//  13 │ │
//  14 │ │       z
//  15 │ ├─▶ }
//     · ╰──── expected `string`, found `int`
//     ╰────
