function f(x: int) -> int {
    match (x) {
        0 => 1,
        _ => return 2, // fallback
    }
}
