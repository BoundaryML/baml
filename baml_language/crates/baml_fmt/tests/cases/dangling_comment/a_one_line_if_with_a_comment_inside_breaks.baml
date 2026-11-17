function f(c: bool, a: int) -> int {
    let q = if (c) { 1 /* c */ } else { 2 };
    let x = if (a > 0) { "a" } /* c */ else { "b" };
    let y = if (c) { 1 } else { 2 };
    spawn { work() }
    defer { c.cleanup() } // runs LIFO
    defer {
        a;
        b
    }
    y
}
