
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
// Error: type mismatch: expected string, got 12 | 13
//     ╭─[ headers_basic_if.baml:14:5 ]
//     │
//  14 │     z
//     │     ┬  
//     │     ╰── type mismatch: expected string, got 12 | 13
//     │ 
//     │ Note: Error code: E0001
// ────╯
