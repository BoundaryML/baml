// An empty-list element adopts from ground siblings
// An empty-list element adopts from its ground siblings (TS
// best common type; rustc unifies vec elements - E0282 fires only with
// NO evidence); the rustc-strict erasure is for genuinely unconstrained
// literals
function el_adopt() -> int throws never {
    let rows = [[1], [2, 3], []];
     // ^^^^ int[][]
    rows.length()
}
