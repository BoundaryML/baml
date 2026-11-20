function f() -> int {
    foo(a, // c
        (x) -> { x });
    foo(a,
        // own line
        (x) -> { x });
    0
}
