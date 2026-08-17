//
// B-1080: https://linear.app/boundaryml2/issue/B-1080
// `baml.Array<T>` / `baml.Map<K, V>` written as annotations ARE the
// structural types: lowering bridges them to List/Map, so every algebra
// arm relates them everywhere (TIR bridges one direction on the
// argument path only; let/return/field positions mismatch). A
// user-defined `class Array<T>` stays nominal.
function f(a: baml.Array<int>) -> int[] {
    a
 // ^ int[]
}

function g(m: baml.Map<string, int>) -> map<string, int> {
    m
 // ^ map<string, int>
}
