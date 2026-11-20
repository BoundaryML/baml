function f(o: int[]?, a: A, b: int) -> int? {
    let v = o // c
        ?.[0];
    let w = (o) // c
        ?.[0];
    let out = a?.(b) // c
        .x.f(z).g(w);
    v
}
