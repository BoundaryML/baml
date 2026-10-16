// Native backend fixture: `throw` of user and stdlib error classes, uncaught.

class Invalid { message: string }
class Detail { n: int, note: string }
class Coded {
    code: int,
    message: string,
    retry: bool,
    ratio: float,
    tags: string[],
    detail: Detail,
    extra: Detail | null,
    counts: map<string, int>,
}

function throw_if_negative(x: int) -> int {
    if (x < 0) { throw Invalid { message: "negative" } }
    x
}

function throw_coded(x: int) -> int {
    if (x < 0) {
        throw Coded {
            code: x,
            message: "coded",
            retry: x == -1,
            ratio: 1.5,
            tags: ["a", "b \"q\""],
            detail: Detail { n: x * 2, note: "d" },
            extra: null,
            counts: { "k": x },
        }
    }
    x * 2
}

function throw_local(x: int) -> int {
    let e = Coded {
        code: x,
        message: "local",
        retry: false,
        ratio: 0.0,
        tags: [],
        detail: Detail { n: 1, note: "" },
        extra: Detail { n: 2, note: "e" },
        counts: {},
    };
    if (x < 0) { throw e }
    x
}

function throw_stdlib(x: int) -> int {
    if (x < 0) { throw baml.errors.InvalidArgument { message: "stdlib" } }
    if (x == 0) { throw baml.json.DecodeError { message: "json", path: ".x" } }
    x
}

function throw_panic_class(x: int) -> int {
    if (x < 0) { throw baml.panics.UserPanic { message: "built by hand" } }
    x
}

function throw_deep(x: int) -> int {
    throw_if_negative(x) + 1
}

function throw_in_loop(n: int) -> int {
    let total = 0;
    for (let i in [1, 2, 3]) {
        if (i == n) { throw Detail { n: i, note: "loop" } }
        total = total + i;
    }
    total
}

function panic_deep(x: int) -> int {
    let xs = [1, 2, 3];
    xs[x] + throw_if_negative(x)
}
