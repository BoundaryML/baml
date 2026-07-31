// B-1010: https://linear.app/boundaryml2/issue/B-1010
// `baml.Array.generate(length, f)` declares f's parameter as `int` in
// the signature, so the unannotated lambda param deduces from that slot
// (S9 deduction through the type-qualified method path). The ticket's
// "cannot infer type of lambda parameter" no longer reproduces in this
// spelling under TIR; this pins the behavior in both engines.
function g() -> int[] {
    baml.Array.generate(3, (i) -> { i })
    //                              ^ int
}
