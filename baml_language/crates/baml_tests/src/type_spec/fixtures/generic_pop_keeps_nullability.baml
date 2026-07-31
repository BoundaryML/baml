// B-633: https://linear.app/boundaryml2/issue/B-633
// `.pop()` on a GENERIC receiver `T[]` keeps its declared `T?` result -
// both engines agree here today. The ticket's LIVE soundness hole is
// the other half: TIR still deems a single-arm `match { let x: T => x }`
// exhaustive over `T | null` with no diagnostic (its fix PR #3904 was
// parked for this rework). That half is an S10 pattern/exhaustiveness
// fixture, `tir: fails`, when match lands.
function f<T>(xs: T[]) -> T? {
    xs.pop()
 // ^^^^^^^^ T | null
}
