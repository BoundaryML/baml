// Runtime counterparts to the compiler-shape fixtures under
// ns_fixtures/ns_short_circuit_locals. The native tests intentionally exercise
// only Opt2; the former duplicate Rust definitions ran the same source once at
// Opt1 and once at Opt2.

function count_while_short_circuiting(items: int[], a: bool, b: bool) -> int {
    let x = false;
    let hits = 0;
    for (let i in items) {
        if (i > 0) {
            x = a && b;
        }
        if (x) {
            hits = hits + 1;
        }
    }
    hits
}

function sum_branch_local(items: int[]) -> int {
    let total = 0;
    for (let i in items) {
        let x = 0;
        if (i > 0) {
            x = 1;
        } else {
            x = 2;
        }
        total = x + total;
    }
    total
}

test "short circuit under a conditional in a loop" {
    assert.equal(count_while_short_circuiting([0, 1, 2, 0, 3], true, true), 4)
}

test "initialized local assigned in both branches of a loop" {
    assert.equal(sum_branch_local([1, -1, 2, -2, 0]), 8)
}
