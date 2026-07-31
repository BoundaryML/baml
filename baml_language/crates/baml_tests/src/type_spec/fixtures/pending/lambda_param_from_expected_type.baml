// pending: S9 lambda params from expectation + I1 operator interfaces
//
// `x` needs S9 (param type from the expected function type); `x + 1`
// needs I1: operators type through the interfaces (baml.ops.Add), never
// a builtin table - MIR rewrites primitive cases to instructions later.
function apply(f: (x: int) -> int throws never, v: int) -> int {
    f(v)
}

function l() -> int {
    apply((x) -> { x + 1 }, 41)
        //         ^ int
        //         ^^^^^ int
}
