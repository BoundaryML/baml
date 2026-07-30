// pending: S11 inference variables for empty containers
//
// doc-inference.md section 3.6: `[]` becomes `List(?e)` with a real
// inference variable; `push` contributes `int` as a lower bound before the
// final use forces resolution. No EvolvingList sentinel, and unlike the
// sentinel this also works through aliases, fields, and temporaries
// (identity lives in the union-find, not a local-name map).
function f() -> int[] {
    let xs = [];
    xs.push(1);
    xs
 // ^^ int[]
}
