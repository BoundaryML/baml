// Iterating arrays whose elements are closures: `ArrayIterator.next` reads
// elements by index rather than gating each on a runtime `T`-test, so a
// fn-typed element is yielded without one. Previously
// only the engine suite exercised these loop-closure shapes; the second test
// pins the companion invariant the old match encoded by arm order: a stored
// null in a nullable element type is yielded, not treated as end-of-iteration.

function closure_loop_sum() -> int {
    let sum = 0;
    let total = [];
    for (let i in [1, 2, 3, 4]) {
        total.push(() -> {
            sum += i;
        })
    }
    for (let cb in total) {
        cb();
    }
    sum
}

test "iterating_closure_array_calls_each" {
    assert.equal(closure_loop_sum(), 10)
}

function nullable_elements_sum() -> int {
    let count = 0;
    for (let v in [1, null, 3]) {
        if (v == null) {
            count += 10;
        } else {
            count += 1;
        }
    }
    count
}

test "iterating_nullable_elements_yields_stored_null" {
    assert.equal(nullable_elements_sum(), 12)
}
