// Runtime regression tests for `baml.ops.Index` (S15 A1): the operator
// path (direct bytecode) and the GENERIC-BOUND path (the stdlib blanket
// impls, whose BAML bodies delegate to the statically-typed operator)
// must agree.

function direct_ops() -> bool {
    let xs = [10, 20, 30];
    let m: map<string, int> = { "a": 1 };
    let bytes = b"AB";
    xs[1] == 20 && m["a"] == 1 && bytes[0] == 65
}

test "index_direct_operators" {
    assert.is_true(direct_ops())
}

function nth<C extends baml.ops.Index<int>>(c: C, i: int) -> C.Output throws never {
    c.index(i)
}

test "index_generic_bound_dispatch" {
    assert.equal(nth<int[]>([10, 20, 30], 2), 30);
    assert.equal(nth<uint8array>(b"AB", 1), 66)
}

function assign_through_index() -> int {
    let xs = [1, 2, 3];
    xs[0] = 9;
    xs[1] += 5;
    xs[0] + xs[1]
}

test "index_assignment" {
    assert.equal(assign_through_index(), 16)
}
