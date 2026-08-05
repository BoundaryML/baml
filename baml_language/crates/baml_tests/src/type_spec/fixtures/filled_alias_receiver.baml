// Structural aliasing at structure points (filled family)
//
// `grid`'s element var has ONE var-carrying lower bound (`row`'s
// still-open list type) and no uppers. rustc unifies that pair the
// moment the argument checks, so a structure-demanding point (the
// index base / method receiver here) commits the occurs-guarded alias
// too - `grid[0]` is `row`'s list and `push` resolves mid-walk.
function filled_alias() -> int[][] throws never {
    let row = baml.Array.filled(2, 0);
    let grid = baml.Array.filled(3, row);
    let n = grid[0].push(99);
     // ^ int
    grid
}
