// a deferred empty-list element re-drives after its target solves
function del_pair<T>(a: T, b: T) -> T throws never {
    a
}
function del_empty_arg() -> int[] throws never {
    let joined = del_pair([1, 2], [])
     // ^^^^^^ int[]
    joined
}
function del_eq<T>(a: T, b: T) -> bool throws never {
    true
}
function del_empty_deep() -> bool throws never {
    let xs = [1, 2]
    let same = del_eq(xs, [])
     // ^^^^ bool
    same
}
