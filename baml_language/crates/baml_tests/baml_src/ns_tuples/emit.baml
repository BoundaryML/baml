
function tpe_int() -> int {
    1
}

function tpe_str() -> string {
    "s"
}

function tpe_carried_calls() -> (int, string) {
    (tpe_int(), tpe_str())
}

function tpe_carried_prefix() -> (int, int) {
    (tpe_int(), 2)
}

function tpe_slot_sum(t: (int, string, int)) -> int {
    let (let a, _, let b) = t;
    a + b
}

function tpe_union_slot_sum(t: (int, bool) | (int, string, float)) -> int {
    match (t) {
        (let a, _) => a + 1,
        (let a, _, _) => a + 2,
    }
}

function tpe_union_slot_mixed(t: (int, bool) | (float, bool)) -> string {
    match t {
        let ints: (int, bool) => "int",
        let floats: (float, bool) => "float",
    }
}

test "tuple_emit_shapes" {
    assert.equal(tpe_carried_calls(), (1, "s"))
    assert.equal(tpe_carried_prefix(), (1, 2))
    assert.equal(tpe_slot_sum((1, "x", 2)), 3)
    assert.equal(tpe_union_slot_sum((1, true)), 2)
    assert.equal(tpe_union_slot_sum((1, "x", 2.5)), 3)
    assert.equal(tpe_union_slot_mixed((1, true)), "int")
    assert.equal(tpe_union_slot_mixed((1.5, true)), "float")
}
