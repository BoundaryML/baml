//# Parent
function SingleNestedChildMultiItems() -> string {
    let items = [1, 2, 3];

    for (let item in items) {
        //## First
        let x = item;

        //## Second
        x
    }

    "done"
}

//----
//- diagnostics
// Error: Expected string, found void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ //# Parent
//    │ │ 
//    │ ╰─ Expected string, found void
//    │ 
//    │ Note: Error code: E0001
// ───╯
