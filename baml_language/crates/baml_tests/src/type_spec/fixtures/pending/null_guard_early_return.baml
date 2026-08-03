// pending: S10b flow narrowing - De Morgan guards + divergence-aware merge
//
// B-688: https://linear.app/boundaryml2/issue/B-688
// The false branch of `a == null || b == null` implies BOTH non-null,
// and the then-branch diverging means the fall-through keeps exactly
// those facts - one ordinary branch-merge rule, no early-return special
// case. TIR never extracts facts from `||` (E0004 on `a + b`).
// tir: fails
function f(a: int?, b: int?) -> int {
    if (a == null || b == null) {
        return 0;
    }
    a + b
 // ^^^^^ int
}
