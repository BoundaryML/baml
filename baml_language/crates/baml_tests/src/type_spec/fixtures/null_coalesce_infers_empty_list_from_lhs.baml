//
// B-1135: https://linear.app/boundaryml2/issue/B-1135
// The unwrapped left operand of `??` INFORMS the right operand, so an
// empty `[]` adopts `int[]` even with no outer context - TIR leaves the
// hole and produces `int[] | _[]`, which then rejects every array
// member on the result. (In a checked position TIR's outside-in
// happens to recover; the context-free binding here is the ticket's
// actual shape.) Informing, not constraining: `v ?? "fallback"` stays
// a join, never a mismatch.
function f(xs: int[]?) -> int[] {
    let ys = xs ?? [];
     // ^^ int[]
    ys
}
