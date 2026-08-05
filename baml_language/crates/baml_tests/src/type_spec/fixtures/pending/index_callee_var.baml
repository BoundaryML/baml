// pending: method call on an obligation-var index base
//
// `baml.Array.generate`'s return var solves from its lambda LOWER bound
// only at finish (resolve_bounded_vars), so a method call whose receiver
// is `grid[0]` mid-walk sees an unsolved Index output. Needs rustc's
// structurally_resolve_type before method lookup.
function grid_push() -> int[][] throws never {
    let grid = baml.Array.generate(3, (r: int) -> int[] { baml.Array.filled(2, 0) });
    let n = grid[0].push(99);
     // ^ int
    grid
}
