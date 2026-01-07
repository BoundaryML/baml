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
// Error: Statement must end with a semicolon.
//     ╭─[ headers_single_nested_child_multi_items.baml:5:5 ]
//     │
//   5 │ ╭─▶     for (let item in items) {
//     ┆ ┆   
//  11 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
