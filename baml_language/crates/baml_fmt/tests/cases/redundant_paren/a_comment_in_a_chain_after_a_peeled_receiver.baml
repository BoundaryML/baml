function f(client: C, x: X, a: A) -> int {
    let v = (client).options // which options
        .with_retry(3);
    let w = (x).fld /* b */ .m0(1);
    let u = (a.b).c // c
        .d;
    v
}
