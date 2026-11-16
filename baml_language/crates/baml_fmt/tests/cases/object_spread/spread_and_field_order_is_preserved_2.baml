class P {
    name: string,
    score: int,
}

function f(p: P, a: P, b: P) -> P {
    P { score: 9, ...p }
}
