//
// doc-inference.md ruling 1: a generic-param var resolves Rust-style - all
// lower bounds must be EQUAL after fresh-literal widening. Both `1` and `2`
// widen to `int`, so `T = int`. (TIR's first-binding-wins call matching
// freezes `T` to the fresh literal `1` and then rejects `2`.)
function pair<T>(a: T, b: T) -> T[] {
    [a, b]
}

function f() -> int[] {
    let p = pair(1, 2);
     // ^ int[]
    p
}
