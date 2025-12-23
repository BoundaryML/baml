//# Parent
function MixedChildrenNotFlattened() -> string {
    //## MD Child
    let items = [42];
    for (let item in items) {
        //# Loop Child
        item
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
