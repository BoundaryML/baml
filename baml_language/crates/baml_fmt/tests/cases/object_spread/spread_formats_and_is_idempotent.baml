class P {
    name: string,
    score: int,
}

function f(p: P) -> P {
    P { ...p, score: 1 }
}
