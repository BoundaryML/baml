//
// (The join lines: TIR joins are dedup-only and keep the member
// alongside its interface; the canonical algebra absorbs.)
//
// I1: the `requires` closure as a fact - `baml.ops.Compare requires
// baml.ops.Equals` (stdlib), so a Compare existential widens to Equals.
function f(c: baml.ops.Compare) -> baml.ops.Equals {
    c
 // ^ baml.ops.Compare
}

// Join-observable: `Compare | Equals` collapses to Equals only if the
// requires closure proves `Compare <: Equals`.
function join_absorbs(c: baml.ops.Compare, e: baml.ops.Equals, b: bool) -> baml.ops.Equals {
    let u = if (b) { c } else { e };
     // ^ baml.ops.Equals
    u
}
