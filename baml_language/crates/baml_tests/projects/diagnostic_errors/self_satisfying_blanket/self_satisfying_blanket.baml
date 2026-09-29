// A blanket impl whose own bound is the interface it provides can satisfy
// itself forever. Confirming it registers its bound as a nested goal, and
// each confirmation instantiates the header with FRESH variables, so the
// chain GROWS rather than repeating a goal -- a cycle check on equality
// would never stop it. The recursion budget does, and the verdict matches
// what the ground resolver gives for the same shape.

interface Cyc<X> {
    function m(self) -> int throws never
}

class Foo { n: int }

implements<X, T extends Cyc<X>> Cyc<X> for T {
    function m(self) -> int throws never { 0 }
}

function needs_cyc<X, T extends Cyc<X>>(x: T) -> int throws never {
    0
}

function go(f: Foo) -> int throws never {
    needs_cyc(f)
}
