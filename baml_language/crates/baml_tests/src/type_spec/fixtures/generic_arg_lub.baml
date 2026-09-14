// generic instantiation solves to the least upper bound of its lowers
function gal_pair<T>(a: T, b: T) -> T throws never {
    a
}
function gal_ret_three() -> 3 throws never {
    3
}
function gal_rigid_join() -> int throws never {
    let x = gal_pair(gal_ret_three(), 3)
     // ^ 3
    x
}
function gal_union_join(flag: bool) -> int[] | string throws never {
    let v: int[] | "x" = if (flag) { [1] } else { "x" }
    let w = gal_pair(v, "x")
     // ^ "x" | int[]
    w
}
test "gal_equals" {
    assert.is_true(gal_ret_three() == 3)
}
