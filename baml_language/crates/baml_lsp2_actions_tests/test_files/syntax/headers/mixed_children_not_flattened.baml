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
// E0010
//
//   × invalid syntax
//    ╭─[headers_mixed_children_not_flattened.baml:1:1]
//  1 │ //# Parent
//    · ─────┬────
//    ·      ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
