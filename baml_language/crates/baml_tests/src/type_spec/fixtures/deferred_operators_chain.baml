//
// (TIR freezes the accumulator to the literal seed - the frozen-
// first-binding family - and errors on the chain.)
//
// I4 stress: obligations chain - the OUTER `+ 2` registers against the
// call's still-unsolved result variable, the inner `a + b` against the
// accumulator; the fulfillment fixpoint discharges inner-then-outer
// once the seed grounds the accumulator to int.
function combine<A>(f: (x: A, y: A) -> A, seed: A) -> A {
    f(seed, seed)
}

function h() -> int {
    let r = combine((a, b) -> { a + b }, 1) + 2;
     // ^ int
    r
}
