
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
//     ╭─[headers_basic_if.baml:14:5]
//  14 │     z
//     ·     ┬
//     ·     ╰── expected `string`, found `int`
//     ╰────
