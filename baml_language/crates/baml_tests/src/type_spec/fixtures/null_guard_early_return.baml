//
// B-688: https://linear.app/boundaryml2/issue/B-688
// The false branch of `a == null || b == null` implies BOTH non-null
// (walk-time De Morgan), and the then branch diverging means the
// fall-through keeps exactly those facts - the ordinary
// divergence-aware branch merge, no early-return special case. TIR
// never extracts facts from `||` (E0004 on `a + b`).
function f(a: int?, b: int?) -> int {
    if (a == null || b == null) {
        return 0;
    }
    a + b
 // ^^^^^ int
}
