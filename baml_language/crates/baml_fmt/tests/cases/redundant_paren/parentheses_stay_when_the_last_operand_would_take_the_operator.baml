function f(a: int, c: int, d: int, x: int?) -> int {
    let v1 = (c | a is int) | d;
    let v2 = (x ?? a is int) ?? d;
    let v3 = !(x is Foo.m());
    v1
}
