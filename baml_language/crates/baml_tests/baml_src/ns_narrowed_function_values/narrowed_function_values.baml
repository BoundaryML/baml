function reordered_combine(x: int, a: int = 10, b: int = 100) -> int {
    (x * 100) + (a * 10) + b
}

function defaulted_combine(x: int, a: int = 10, b: int = 100) -> int {
    x + a + b
}

function apply_concrete(f: (x: int, b?: int) -> int) -> int {
    f(1, b = 5)
}

function apply_generic<T>(f: (x: T, b?: int) -> T, value: T) -> T {
    f(value, b = 5)
}

test "narrowed function value reorders named optionals" {
    let f: (x: int, b?: int, a?: int) -> int throws never = reordered_combine;
    assert.equal(f(1, b = 5, a = 2), 125)
}

test "narrowed function value is accepted as a concrete call argument" {
    assert.equal(apply_concrete(defaulted_combine), 16)
}

test "narrowed function value is accepted as a generic call argument" {
    assert.equal(apply_generic(defaulted_combine, 1), 16)
}
