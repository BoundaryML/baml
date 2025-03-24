fn Foo(x: int) -> int {
    x
}

fn Bar(x: int, y: string) -> int {
    let y = go(x);
    let z = go2(x,y);
    go3(z)
}

test TestBar {
    functions [Bar]
    args {
        x 1
        y "hello"
    }
}