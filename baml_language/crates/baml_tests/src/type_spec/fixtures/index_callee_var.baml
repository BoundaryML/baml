// Structural resolution (rustc's structurally_resolve_type): a method
// call whose receiver is a still-unsolved bounded var mid-walk
// (`baml.Array.generate`'s return var, lower-bounded by its lambda) is
// forced from the bounds accumulated so far at the structure-demanding
// point, so `grid[0].push(99)` resolves during the walk instead of
// erroring before finish-time resolution.
function grid_push() -> int[][] throws never {
    let grid = baml.Array.generate(3, (r: int) -> int[] { baml.Array.filled(2, 0) });
    let n = grid[0].push(99);
     // ^ int
    grid
}
