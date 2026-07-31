// B-633: https://linear.app/boundaryml2/issue/B-633
// `.pop()` on a GENERIC receiver `T[]` keeps its declared `T?` result.
// The ticket's soundness hole (TIR erased the `?`, letting null leak
// into a non-nullable `T`) no longer reproduces in this spelling - this
// pins the correct behavior against regression in either engine.
function f<T>(xs: T[]) -> T? {
    xs.pop()
 // ^^^^^^^^ T | null
}
