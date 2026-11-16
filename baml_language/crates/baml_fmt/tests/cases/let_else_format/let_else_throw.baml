class NoMatch {
}

function f(r: int | string) -> int throws NoMatch {
    let n: int = r else {
        throw NoMatch {};
    };
    n
}
