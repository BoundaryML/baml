fn Foo(x: int) -> int {
    let y = x;
    x
}

test TestBar {
    functions [Foo]
    args {
        x 1
    }
}