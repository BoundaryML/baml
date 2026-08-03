// I4: operator dispatch on a still-unsolved generic REGISTERS an
// obligation (fresh output var stands for the result) and discharges
// at the fulfillment fixpoint once the accumulator grounds - the
// rust-analyzer register-and-fulfill discipline, with bound resolution
// deciding from the ground subset so the output-bounds-the-operand
// cycle cannot deadlock.
function combine<A>(f: (x: A, y: A) -> A, seed: A) -> A {
    f(seed, seed)
}

function g() -> int {
    combine((a, b) -> { a + b }, 0)
              //        ^^^^^ int
}
