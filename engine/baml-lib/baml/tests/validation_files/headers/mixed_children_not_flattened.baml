# Parent
function MixedChildrenNotFlattened() -> string {
    ## MD Child
    let items = [42];
    for (item in items) {
        ### Loop Child
        ### And stuff
        item
    }
    "done"
}
