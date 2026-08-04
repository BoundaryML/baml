// pending: catch-arm expectation propagation (empty-literal adoption in catch joins)
//
// tir: fails
//
// (hir_ty today: the arm's element var never meets the return
// expectation - the catch join is built before the return check, and
// the var erases to `int[] | !error[]`.)
//
// B-616 (https://linear.app/boundaryml2/issue/B-616): an empty `[]` in
// a `catch_all` arm unifies with the try-block's element type instead
// of forming `int[] | _[]`. hir_ty: the arm's element var joins the
// catch join syntactically and the return check resolves it upward.
class Io {
    message string
}

function docs() -> int[] throws user.Io {
    throw Io { message: "x" }
}

function use_catch() -> int[] throws never {
    let r = docs() catch_all (e) { _ => [] };
     // ^ int[]
    r
}
