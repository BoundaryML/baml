//
// B-616 (https://linear.app/boundaryml2/issue/B-616): an empty `[]` in
// a `catch_all` arm unifies with the try-block's element type instead
// of forming `int[] | _[]` (TIR's shape - the ticket's "cannot iterate
// over int[] | _[]"). hir_ty: the arm's element var joins the catch
// union syntactically, and the return check DECOMPOSES the union
// member-wise (`A | B <: C` iff both are - set semantics), so `?E[]`
// meets `int[]` and solves instead of deferring and erasing.
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
