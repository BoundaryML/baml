// B-1082 (https://linear.app/boundaryml2/issue/B-1082): a type variable
// in a `throws` clause must NOT disable the contract check - `Kaboom` is
// not covered by `E` (the check defers through bounds, never skips), so
// the escape records on the error channel. TIR skips the entire check
// when the clause carries a TypeVar - the ticket's bug - so it reports
// nothing; its silence is invisible here (the mismatch annotation is a
// hir_ty-channel assertion), making this an agreement fixture on types
// and a hir_ty-only pin on the error.
class Kaboom {
    m string
}

function f<E>(cb: () -> int throws E) -> int throws E {
    throw Kaboom { m: "x" };
  //      ^^^^^^^^^^^^^^^^^ user.Kaboom
  //      ^^^^^^^^^^^^^^^^^ mismatch: expected E, got user.Kaboom
    cb()
}
