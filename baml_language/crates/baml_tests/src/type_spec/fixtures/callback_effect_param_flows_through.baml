//
// (TIR diverges via thrown-literal widening: it infers `throws
// string` where the spec-ahead behavior keeps `"neg"`.)
//
// TYPE_SYSTEM.md Functions rule 4: a function-typed PARAMETER may omit
// its throws clause - elaboration mints an implicit effect generic on
// the surrounding function - and rule 3 means the lambda passed to it
// needs no declaration either: its body-inferred throws fills the
// effect param, and the caller's channel sees exactly that.
function apply(f: (x: int) -> int, v: int) -> int {
    f(v)
}

function g(v: int) -> int {
      // ^ throws "neg"
    apply((x) -> { if (x < 0) { throw "neg"; } x }, v)
}
