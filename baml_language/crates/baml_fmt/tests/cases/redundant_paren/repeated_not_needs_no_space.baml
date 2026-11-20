function f(x: bool) -> bool {
    let a = !!x;
    let b = !(!x);
    a && b
}
