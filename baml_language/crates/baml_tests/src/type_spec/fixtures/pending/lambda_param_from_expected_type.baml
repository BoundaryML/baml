// pending: S9 lambda params from expectation
function apply(f: (x: int) -> int throws never, v: int) -> int {
    f(v)
}

function l() -> int {
    apply((x) -> { x + 1 }, 41)
        //         ^ int
        //         ^^^^^ int
}
