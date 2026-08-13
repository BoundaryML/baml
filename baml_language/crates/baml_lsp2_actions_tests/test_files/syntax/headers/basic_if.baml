
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
// E0010
//
//   × invalid syntax
//    ╭─[headers_basic_if.baml:1:1]
//  1 │ //# Basic test
//    · ───────┬──────
//    ·        ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[headers_basic_if.baml:14:5]
//  14 │     z
//     ·     ┬
//     ·     ╰── expected `string`, found `int`
//     ╰────
