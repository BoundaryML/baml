
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
// Error: Expected `string`, found `int`
//     ╭─[ headers_basic_if.baml:14:5 ]
//     │
//  14 │     z
//     │     ┬  
//     │     ╰── Expected `string`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
