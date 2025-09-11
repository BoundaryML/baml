function Foo(x: int) -> int {
    let y = x;
    x
}

function Bar(x: int) -> int {
    // test
    let y = x;
    x
}

test TestBar {
    functions [Foo]
    args {
        x 1
    }
}
