// B-1576: an `if`/`else` join of a plain array and an `Array.map` lambda
// happens while the map's element var is unsolved, so the binding records a
// SYNTACTIC union of lists. The for-loop's element demand forces the var and
// must then judge the CANONICAL spelling: the union collapses to `int[]` and
// `Iterable.Item` reduces to `int`. 0.17.0 left the element the error
// sentinel with zero diagnostics and aborted in runtime lowering.

function joined_map_arm(xs: int[], b: bool) -> int {
    let ys = if (b) {
        [0]
    } else {
        xs.map((x: int) -> int {
            x + 1
        })
    };
    let n = 0;
    for (let y in ys) {
    //       ^ int
    //            ^^ int[]
        n = n + y;
    //          ^ int
    }
    n
}

// The rigid half of the same defect: over a bounded generic collection the
// element is the projection `(T as Iterable).Item` - irreducible at compile
// time by design (resolved per-receiver at runtime), never an error.

function count_bounded<T extends baml.iter.Iterable>(c: T) -> int {
    let n = 0;
    for (let item in c) {
    //       ^^^^ (T as baml.iter.Iterable).Item
        n = n + 1;
    }
    n
}
