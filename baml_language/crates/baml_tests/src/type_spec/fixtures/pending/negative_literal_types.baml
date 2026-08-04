// pending: A9 negative literal types (S15 ledger, ~250 nodes; ruling 2)
//
// `-1` IS a literal type (TS parity; TIR agrees): unary minus applied
// directly to a numeric literal folds to a negative FRESH literal,
// widening to the base at bindings like any literal. `-x` on
// non-literals stays Negate dispatch.
function f(c: bool) -> int throws never {
    let k: -1 = -1;
     // ^ -1
    let m = -1;
     // ^ int
    let j = if (c) { 1 } else { -1 };
     // ^ int
    m + j
}
