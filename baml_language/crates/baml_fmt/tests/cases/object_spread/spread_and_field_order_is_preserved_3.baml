class P {
    name: string,
    score: int,
}

function f(p: P, a: P, b: P) -> P {
    P { ...a, ...b }
}
