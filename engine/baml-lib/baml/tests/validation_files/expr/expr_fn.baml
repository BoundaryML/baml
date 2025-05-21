function Foo(x: int) -> int {
    let y = x;
    x
}

fn Bar(x: int) -> int {
    let y = x;
    x
}

test TestBar {
    functions [Foo]
    args {
        x 1
    }
}