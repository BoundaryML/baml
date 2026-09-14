//
// B-1134 / B-742 / B-267.1: https://linear.app/boundaryml2/issue/B-1134
// The literal seed of `reduce` is a fresh-literal LOWER BOUND on the
// accumulator generic, widening to `int` under ruling 1 - TIR pins the
// singleton `0` and then rejects the lambda's `int` against it.
// (`a + b` inside the lambda stays the sentinel until I4 defers
// operator dispatch on unsolved vars; the accumulator still resolves.)
function f() -> int {
    let total = [1, 2, 3].reduce((a, b) -> { a + b }, 0);
     // ^^^^^ int
    total
}
