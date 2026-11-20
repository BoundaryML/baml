// BAML has no increment or decrement operators: `--x` is two negations,
// `++x` and `x++` are errors at the `+` that begins no expression.
function f(x: int) -> int {
    let a = --x;
    let b = ++x;
    let c = x++;
    a + b + c
}
