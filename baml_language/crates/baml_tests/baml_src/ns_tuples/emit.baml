// Bytecode shapes for tuple construction and slot reads: elements carried on
// the stack into `AllocTuple`, and slot reads typed precisely enough for
// arithmetic to specialize. Names carry a `tpe_` prefix.

function tpe_int() -> int {
    1
}

function tpe_str() -> string {
    "s"
}

// Both elements are call results. They stay on the stack, beneath the tuple
// descriptor's `LoadType`, until `AllocTuple` consumes them.
function tpe_carried_calls() -> (int, string) {
    (tpe_int(), tpe_str())
}

// A carried call result followed by a constant element.
function tpe_carried_prefix() -> (int, int) {
    (tpe_int(), 2)
}

// Pattern projections read slots at constant indices, so the sum of two
// `int` slots specializes.
function tpe_slot_sum(t: (int, string, int)) -> int {
    let (let a, _, let b) = t;
    a + b
}

// Every arm's slot 0 is an `int`, across arities.
function tpe_union_slot_sum(t: (int, bool) | (int, string, float)) -> int {
    match (t) {
        (let a, _) => a + 1,
        (let a, _, _) => a + 2,
    }
}

// Slot 0 differs across arms: the read joins to `int | float`, which must
// not take an `int`-only path.
function tpe_union_slot_mixed(t: (int, bool) | (float, bool)) -> string {
    match (t[0]) {
        let i: int => "int",
        let f: float => "float",
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
