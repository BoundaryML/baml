// pending: A1 index expressions via baml.ops.Index (S15 ledger, 804 nodes)
//
// Ruling: indexing dispatches through a `baml.ops.Index` interface,
// Rust's `ops::Index` shape (stdlib interface + Output; MIR rewrites
// primitive cases to instructions). Until it lands, `a[i]` types
// `!error`.
function f(xs: int[], i: int) -> int throws never {
    let a = xs[0];
     // ^ int
    let b = xs[i];
     // ^ int
    xs[0] = 7;
    a + b
}
