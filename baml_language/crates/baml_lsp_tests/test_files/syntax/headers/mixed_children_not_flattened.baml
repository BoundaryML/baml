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
// Error: Expected `0`, found `1`
//    ╭─[ headers_mixed_children_not_flattened.baml:1:1 ]
//    │
//  1 │ //# Parent
//    │ │ 
//    │ ╰─ Expected `0`, found `1`
//    │ 
//    │ Note: Error code: E0001
// ───╯
