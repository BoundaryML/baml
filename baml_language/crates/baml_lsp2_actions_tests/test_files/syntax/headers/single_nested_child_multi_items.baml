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
// E0010
//
//   × invalid syntax
//    ╭─[headers_single_nested_child_multi_items.baml:1:1]
//  1 │ //# Parent
//    · ─────┬────
//    ·      ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
