//
// S12: an omitted `throws` clause infers from the body - throw sites
// keep their literal types and union with callee effects (fixpoint over
// mutual recursion); the callee reference renders the inferred effect.
function f(c: bool) -> int {
    if (c) {
        throw "bad";
    }
    1
}

function g(c: bool) -> int {
    f(c)
 // ^ (c: bool) -> int throws "bad"
}
