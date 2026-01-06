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
// Error: Expected `0`, found `1`
//    ╭─[ headers_single_nested_child_multi_items.baml:1:1 ]
//    │
//  1 │ //# Parent
//    │ │ 
//    │ ╰─ Expected `0`, found `1`
//    │ 
//    │ Note: Error code: E0001
// ───╯
