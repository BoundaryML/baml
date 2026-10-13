// Regression for B-1788: a method called on the result of an operator whose
// operand type is still being inferred (here, the return of a generic call)
// panicked during emission with "indirect calls require an explicit caller
// layout". Method lookup ran before the pending operator was resolved, so no
// method or call plan was recorded.
function id<T>(x: T) -> T {
    x
}

function negated_abs(n: int) -> int {
    (-id(n)).abs()
}

test "method on unary operator over generic result" {
    assert.equal(negated_abs(3), 3)
}

function incremented_abs(n: int) -> int {
    (id(n) + 1).abs()
}

test "method on binary operator over generic result" {
    assert.equal(incremented_abs(-5), 4)
}

// The originally reported shape: a reduce accumulator is bounded by its own
// callback's output, so resolving the receiver must break that cycle.
function rounded_total(values: int[]) -> float {
    let total = values.reduce(
        (sum, n) -> {
            sum + n
        },
        0,
    );
    (total * 1.0).round()
}

test "method on operator over reduce accumulator" {
    assert.equal(rounded_total([1, 2]), 3.0);
    assert.equal(rounded_total([]), 0.0)
}

test "method on operator through nested generic aliases" {
    let later = [];
    assert.equal(id(id(-id(3))).abs(), 3);
    // Demanding the receiver must not finalize unrelated inference variables.
    later.push("hello");
    assert.equal(later[0], "hello");
}
