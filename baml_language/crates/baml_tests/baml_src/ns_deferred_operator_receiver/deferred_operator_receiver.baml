// Regression for B-1788: an inherent method on an operator result whose
// operand comes from a generic still being inferred. Method lookup ran before
// the operator resolved, typed the call `Error` with no diagnostic, and
// emission panicked with "indirect calls require an explicit caller layout".
// One case per way a generic can leave its result bounded but unsolved.
function id<T>(x: T) -> T {
    x
}

class Box<T> {
    value: T,
}

function from_generic_function(x: float) -> float {
    (id(x) * 100.0).round()
}

test "inherent method on operator over generic function result" {
    assert.equal(from_generic_function(0.123), 12.0)
}

function from_generic_class(n: int) -> int {
    (-Box { value: n }.value).abs()
}

test "inherent method on operator over generic class field" {
    assert.equal(from_generic_class(3), 3)
}

function from_untyped_lambda(s: string) -> string {
    let g = (a) -> {
        a
    };
    (g(s) + " ").trim()
}

test "inherent method on operator over untyped lambda result" {
    assert.equal(from_untyped_lambda(" hi"), "hi")
}

function from_generic_method(values: int[]) -> int {
    (
        -values.reduce(
            (sum, n) -> {
                sum + n
            },
            0,
        )
    )
        .abs()
}

test "inherent method on operator over generic method result" {
    assert.equal(from_generic_method([1, 2]), 3)
}

test "resolving a receiver leaves unrelated inference deferred" {
    let later = [];
    assert.equal(id(id(-id(3))).abs(), 3);
    later.push("hello");
    assert.equal(later[0], "hello");
}
