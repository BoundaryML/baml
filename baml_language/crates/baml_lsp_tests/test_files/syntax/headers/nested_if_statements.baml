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
// Error: Expected string, found int
//    ╭─[ headers_nested_if_statements.baml:1:1 ]
//    │
//  1 │ //# Main Function
//    │ │ 
//    │ ╰─ Expected string, found int
//    │ 
//    │ Note: Error code: E0001
// ───╯
