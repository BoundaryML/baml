//# Case 1
function IfThenElse() -> int {
    let x = 1;

    //# Before 

    //# If should always do else
    if (false) { 
        //## impossible: return 1
        1
    } else {
        //## should always be 5
        5
    }

    //# After
    y
}


//# Main Function
function ComplexHeaderTest(x: int) -> int {
    let z = 5;
    //# oopsie

    //# standalone will render

    //# will not render
    let k = [1, 2, 3];
    //# will not render
    5;
    //# will not render
    true;
    //# will not render
    k[0];

    //# increment maybe?
    if (x < 10) {
        //## increment x
        x = x + 10;
    } else {
        //## dummy variable y
        let y = 5;
    }
    
    //# another increm
    x + 1234
}

//----
//- diagnostics
// Error: Statement must end with a semicolon.
//     ╭─[ headers_if_then_else.baml:8:5 ]
//     │
//   8 │ ╭─▶     if (false) {
//     ┆ ┆   
//  14 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Unknown variable `y`
//     ╭─[ headers_if_then_else.baml:17:5 ]
//     │
//  17 │     y
//     │     ┬  
//     │     ╰── Unknown variable `y`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ headers_if_then_else.baml:38:5 ]
//     │
//  38 │ ╭─▶     if (x < 10) {
//     ┆ ┆   
//  44 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
