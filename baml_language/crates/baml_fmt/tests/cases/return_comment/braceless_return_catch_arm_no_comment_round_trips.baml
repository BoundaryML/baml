function f(x: int) -> int {
    let v = g(x) catch (e) {
        _ => return -1,
    };
    v
}
