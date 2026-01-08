//# Case 1
function IfThenOnlyHeader() -> int {
    let x = 1;

    //# Before 

    //# If
    let y = if (false) { 1 };

    //## After
    y
}

//# Case 2
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //## Before

    let z = 5;

    //### If
    let y = if (false) { 
        //## Then
        1
    };

    //## After
    y
}


//# Case 3
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //### Before

    let z = 5;

    //# If
    let y = if (false) { 
        //## Then
        1
    };

    //## After
    y
}

//----
//- diagnostics
// Error: Duplicate function `IfThenOnlyHeader2`
//    ╭─[ headers_if_then_only_header.baml:1:1 ]
//    │
//  1 │ //# Case 1
//    │ │ 
//    │ ╰─ function `IfThenOnlyHeader2` redefined here
//    │ │ 
//    │ ╰─ `IfThenOnlyHeader2` first defined in headers_if_then_only_header.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Expected `int`, found `int | null`
//     ╭─[ headers_if_then_only_header.baml:28:5 ]
//     │
//  28 │     y
//     │     ┬  
//     │     ╰── Expected `int`, found `int | null`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int`, found `int | null`
//     ╭─[ headers_if_then_only_header.baml:28:5 ]
//     │
//  28 │     y
//     │     ┬  
//     │     ╰── Expected `int`, found `int | null`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int`, found `int | null`
//     ╭─[ headers_if_then_only_header.baml:11:5 ]
//     │
//  11 │     y
//     │     ┬  
//     │     ╰── Expected `int`, found `int | null`
//     │ 
//     │ Note: Error code: E0001
// ────╯
