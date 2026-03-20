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
// Error: unresolved name: item
//    ╭─[ headers_mixed_children_not_flattened.baml:7:9 ]
//    │
//  7 │         item
//    │         ──┬─  
//    │           ╰─── unresolved name: item
//    │ 
//    │ Note: Error code: E0001
// ───╯
