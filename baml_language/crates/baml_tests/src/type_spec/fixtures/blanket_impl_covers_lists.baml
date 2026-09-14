//
// (The join lines: TIR joins are dedup-only and keep the member
// alongside its interface; the canonical algebra absorbs.)
//
// I1: a stdlib BLANKET impl (`implement<T> Equals for T[]`) resolves
// for a concrete list receiver - the bare-blanket concreteness guard
// admits `int[]`, binds `T := int`, and the existential assignment
// holds.
function f(xs: int[]) -> baml.ops.Equals {
    let e: baml.ops.Equals = xs;
     // ^ baml.ops.Equals
    e
}

// Join-observable: `int[] | Equals` collapses to the existential only
// if the blanket impl proves `int[] <: Equals`.
function join_absorbs(xs: int[], e: baml.ops.Equals, c: bool) -> baml.ops.Equals {
    let u = if (c) { xs } else { e };
     // ^ baml.ops.Equals
    u
}
