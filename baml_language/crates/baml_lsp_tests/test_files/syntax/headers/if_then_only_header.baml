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
// Error: Duplicate function 'IfThenOnlyHeader2'
//    ╭─[ headers_if_then_only_header.baml:1:1 ]
//    │
//  1 │ //# Case 1
//    │ │ 
//    │ ╰─ function 'IfThenOnlyHeader2' defined in headers_if_then_only_header.baml
//    │ │ 
//    │ ╰─ 'IfThenOnlyHeader2' previously defined in headers_if_then_only_header.baml
//    │ 
//    │ Note: Error code: E0011
// ───╯
// Error: Expected int, found int | null
//    ╭─[ headers_if_then_only_header.baml:1:1 ]
//    │
//  1 │ //# Case 1
//    │ │ 
//    │ ╰─ Expected int, found int | null
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected int, found int | null
//    ╭─[ headers_if_then_only_header.baml:1:1 ]
//    │
//  1 │ //# Case 1
//    │ │ 
//    │ ╰─ Expected int, found int | null
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected int, found int | null
//    ╭─[ headers_if_then_only_header.baml:1:1 ]
//    │
//  1 │ //# Case 1
//    │ │ 
//    │ ╰─ Expected int, found int | null
//    │ 
//    │ Note: Error code: E0001
// ───╯
