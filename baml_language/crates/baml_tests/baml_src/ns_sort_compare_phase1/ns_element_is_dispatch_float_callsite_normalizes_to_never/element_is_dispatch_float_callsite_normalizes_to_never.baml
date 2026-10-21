function is_fast<T extends baml.ops.Compare>(xs: T[]) -> bool throws never {
    return xs.length() == 0;
}

function fast_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    return xs;
}

function slow_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    return xs;
}

function dispatch_f<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    if (is_fast(xs)) {
        fast_g(xs)
    } else {
        slow_g(xs)
    }
}

function use_float(xs: float[]) -> float[] throws never {
    dispatch_f(xs)
}

test "element_is_dispatch_float_callsite_normalizes_to_never" {
    assert.equal(1, 1)
}
