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
// Error: Statement must end with a semicolon.
//    ╭─[ headers_mixed_children_not_flattened.baml:5:5 ]
//    │
//  5 │ ╭─▶     for (let item in items) {
//    ┆ ┆   
//  8 │ ├─▶     }
//    │ │           
//    │ ╰─────────── Statement must end with a semicolon.
//    │     
//    │     Note: Error code: E0028
// ───╯
