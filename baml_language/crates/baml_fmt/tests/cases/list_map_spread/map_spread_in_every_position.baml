function first(m: map<string, int>) -> map<string, int> {
    {...m,"type":3}
}

function middle(m: map<string, int>) -> map<string, int> {
    { "a": 1,... m, "z": 26 }
}

function last(m: map<string, int>) -> map<string, int> {
    { "a": 1, ...m, }
}

function only(m: map<string, int>) -> map<string, int> {
    {...m}
}

function several(a: map<string, int>, b: map<string, int>) -> map<string, int> {
    map { ...a, ...b, "k": 1, ...a }
}

function nested(a: map<string, int>) -> map<string, map<string, int>> {
    { "outer": { ...a, "inner": 1 }, ...{ "copy": { ...a } } }
}

function as_argument(a: map<string, int>) -> int {
    count({ ...a })
}
