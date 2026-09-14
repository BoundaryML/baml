// `unknown` is a first-class type the author wrote, not an error cascade:
// it implements no operator, so dispatching one on it is a real E0004.
// The RESULT still suppresses to the error sentinel (diagnose-then-fill),
// so nothing downstream cascades. Regression guard: the TyKind->InferTy
// conversion once added Unknown to the diagnostic screen, silently
// accepting these.

function add_unknown(x: unknown) -> string {
    x + "!"
}

function add_unknown_rhs(x: unknown) -> string {
    "!" + x
}
