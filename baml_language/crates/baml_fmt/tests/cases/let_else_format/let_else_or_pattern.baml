class Ok {
    value: string,
}

class Warn {
    value: string,
}

class Err {
    message: string,
}

function f(r: Ok | Warn | Err) -> string {
    let s: Ok | Warn = r else {
        return "err";
    };
    s.value
}
