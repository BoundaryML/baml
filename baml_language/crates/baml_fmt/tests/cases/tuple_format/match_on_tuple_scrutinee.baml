function f(a: bool, b: bool) -> int {
    match (a, b) {
        (true, true) => 1,
        (false, _) | (_, false) => 0,
    }
}

function g(a: bool) -> int {
    match ( a , ) {
        (true,) => 1,
        _ => 0,
    }
}
