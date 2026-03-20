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
// Error: unresolved name: item
//    ╭─[ headers_single_nested_child_multi_items.baml:7:17 ]
//    │
//  7 │         let x = item;
//    │                 ──┬─  
//    │                   ╰─── unresolved name: item
//    │ 
//    │ Note: Error code: E0001
// ───╯
