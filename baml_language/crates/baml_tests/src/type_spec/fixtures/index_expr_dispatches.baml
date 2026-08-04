// A1 (S15 ledger; ruled): indexing dispatches through `baml.ops.Index`,
// Rust's `ops::Index` shape - stdlib blanket impls for `T[]`,
// `map<K, V>`, and `uint8array` whose bodies delegate to the
// statically-typed operator (MIR rewrites those to direct
// instructions, the `Equals for T[]` discipline). Index ASSIGNMENT
// checks the value against the dispatched element type.
function f(xs: int[], i: int) -> int throws never {
    let a = xs[0];
     // ^ int
    let b = xs[i];
     // ^ int
    xs[0] = 7;
    a + b
}
