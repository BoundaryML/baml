// pending: I4 obligations - operator dispatch on an unsolved generic
//
// The documented hole: when the lambda body is walked, `a` and `b` are
// the still-unsolved `?A` (the seed has only deposited a bound), so
// `a + b` cannot consult the impl registry and sentinels. The
// principled fix is I4's probe-vs-register discipline: dispatch
// REGISTERS `Implements(?A, baml.ops.Add<?A>) { Output = ?O }`, types
// the expression `?O`, and discharges once `?A` grounds to int -
// requiring the ground-subset bound resolution interleaved with the
// obligation worklist to fixpoint (the deadlock `?A waits on ?O waits
// on ?A` is why this is I4's machinery and not a point fix).
function combine<A>(f: (x: A, y: A) -> A, seed: A) -> A {
    f(seed, seed)
}

function g() -> int {
    combine((a, b) -> { a + b }, 0)
              //        ^^^^^ int
}
