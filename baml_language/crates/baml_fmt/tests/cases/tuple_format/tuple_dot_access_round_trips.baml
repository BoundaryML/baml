function f(t: ((int, string), bool)) -> string {
    t.0.1
}

function g(t: (int, string)?) -> string? {
    t?.1
}
