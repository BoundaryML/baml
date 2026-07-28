// Rigid-typevar match arms that are PROVABLY dead — no realization of the
// in-scope type variables gives the pattern and the scrutinee a common value —
// are type errors, like any concrete pattern/scrutinee mismatch. (Before the
// overlap-oracle narrowing fix these compiled and silently never matched.)
// The diagnostics snapshot is the assertion. The trailing `_` arms stay
// statically reachable (a rigid arm never covers a concrete member).

interface DeadMarked {
    function tag(self) -> string throws never
}

// `T extends DeadMarked` refutes every member: neither `string` nor `int`
// implements `DeadMarked`, so no realization of `T` can match.
function dead_bounded<T extends DeadMarked>(x: string | int) -> string {
    match (x) {
        let t: T => "T",
        _ => "other",
    }
}

class DeadPair<A, B> {
    a: A
    b: B
}

// One `T` cannot realize to both `int` and `string`: binding consistency
// refutes the arm against every value of the scrutinee type.
function dead_inconsistent<T>(x: DeadPair<int, string>) -> string {
    match (x) {
        let p: DeadPair<T, T> => "pair",
        _ => "other",
    }
}
