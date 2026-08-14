// Value generics through lambdas, direction 1: a sibling argument
// bounds `T` (pass one of the two-pass argument order), the lambda's
// unannotated param takes the `T` slot, and everything resolves at
// finalize under ruling 1 - `x` and the result land on int, not the
// pinned literal.
function tap<T>(v: T, f: (x: T) -> T) -> T {
    f(v)
}

function g() -> int {
    let r = tap(1, (x) -> { x });
     // ^ int
    r
}
