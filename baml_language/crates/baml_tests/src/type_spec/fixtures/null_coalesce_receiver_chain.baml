//
// B-1135 (receiver position): https://linear.app/boundaryml2/issue/B-1135
// `(m.at(u) ?? []).at(v)` on an int[][]: the ?? result is a clean int[]
// receiver (the [] adopted from the unwrapped lhs), so the chained .at
// resolves - TIR's int[] | _[] union rejects every array member.
function f(m: int[][], u: int, v: int) -> int? {
    (m.at(u) ?? []).at(v)
 // ^^^^^^^^^^^^^^^^^^^^^ int | null
}
