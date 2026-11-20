function f(x: int?, a: int, b: int) -> int {
    let k = x ?? // k1
        // k2
        0;
    let l = a + /* a */
        /* b */ /* c */ b;
    let m = a - // a
        b;
    let n = a + /* a */ b;
    k
}
