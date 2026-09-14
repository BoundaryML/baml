// Unannotated lambda params deduce from the expected function type
// (S9); `x + 1` types through baml.ops.Add dispatch, never a builtin
// table (decision 3B) - MIR rewrites primitive cases to instructions.
function apply(f: (x: int) -> int throws never, v: int) -> int {
    f(v)
}

function l() -> int {
    apply((x) -> { x + 1 }, 41)
        //         ^ int
        //         ^^^^^ int
}
