function f(x: int) -> int {
    let z = outer(inner(x, (y) -> {
        let zzz = y;
        zzz
    }));
    z
}
