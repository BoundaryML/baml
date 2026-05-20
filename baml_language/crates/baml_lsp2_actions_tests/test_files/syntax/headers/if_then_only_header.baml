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
// error: `if` without `else` cannot be used as a value; add an `else` branch
//    ╭─[ headers_if_then_only_header.baml:8:12 ]
//    │
//  8 │     let y = if (false) { 1 };
//    │            ────────┬────────  
//    │                    ╰────────── `if` without `else` cannot be used as a value; add an `else` branch
//    │ 
//    │ Note: Error code: E0001
// ───╯
// error: `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[ headers_if_then_only_header.baml:11:5 ]
//     │
//  11 │     y
//     │     ┬  
//     │     ╰── `if` without `else` cannot be used as a value; add an `else` branch
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[ headers_if_then_only_header.baml:22:12 ]
//     │
//  22 │ ╭─▶     let y = if (false) {
//     ┆ ┆   
//  25 │ ├─▶     };
//     │ │            
//     │ ╰──────────── `if` without `else` cannot be used as a value; add an `else` branch
//     │     
//     │     Note: Error code: E0001
// ────╯
// error: `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[ headers_if_then_only_header.baml:28:5 ]
//     │
//  28 │     y
//     │     ┬  
//     │     ╰── `if` without `else` cannot be used as a value; add an `else` branch
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[ headers_if_then_only_header.baml:40:12 ]
//     │
//  40 │ ╭─▶     let y = if (false) {
//     ┆ ┆   
//  43 │ ├─▶     };
//     │ │            
//     │ ╰──────────── `if` without `else` cannot be used as a value; add an `else` branch
//     │     
//     │     Note: Error code: E0001
// ────╯
// error: `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[ headers_if_then_only_header.baml:46:5 ]
//     │
//  46 │     y
//     │     ┬  
//     │     ╰── `if` without `else` cannot be used as a value; add an `else` branch
//     │ 
//     │ Note: Error code: E0001
// ────╯
