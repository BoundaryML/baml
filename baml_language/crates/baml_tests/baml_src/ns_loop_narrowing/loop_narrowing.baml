// A loop's exits are the condition's false edge and every `break`; the code
// after the loop is typed with their merge, and a C-style step slot with the
// merge of the body's end and every `continue`. The compile errors for the
// shapes this rejects are in the diagnostics project
// `diagnostic_errors/loop_narrowing`.
//
// Every case body is a top-level function; a `test` block only asserts.

class LnTag {
    name: string,
}

class LnBag {
    items: int[],
}

// The break edge carries the body's write; the false edge the condition's.
function ln_break_edge_value(a: int) -> string {
    let x: int | LnTag = a;
    while ({ x = a; a > 0 }) {
        x = LnTag { name: "tag" };
        break;
    }
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

// A `break` as the only exit: the value written before it holds after.
function ln_break_only_exit(a: int) -> int {
    let x: int? = null;
    while (true) {
        x = a;
        break;
    }
    x + 1
}

// The false edge of `x is int` leaves the residual; the break edge agrees.
function ln_is_test_exit(a: int, s: string) -> string {
    let x: int | string = a;
    while (x is int) {
        x = s;
        break;
    }
    x
}

// A for-in collection runs once, in the entry flow: a null check before the
// loop holds for the collection even when the body reassigns the binding.
function ln_collection_in_entry_flow(bag: LnBag?) -> int {
    if (bag == null) {
        return 0;
    }
    let total = 0;
    for let v in bag.items {
        total += v;
        bag = null;
    }
    total
}

// The step slot runs after the body's end and after every `continue`.
function ln_step_after_continue(n: int) -> int {
    let x: int | string = 0;
    let strings_at_step = 0;
    for (let i = 0; i < n; strings_at_step += match (x) { string => 1, int => 0 }) {
        i += 1;
        if (i == 1) {
            x = "s";
            continue;
        }
        x = i;
    }
    strings_at_step
}

// A `continue` on the last iteration of a `for let` is an exit edge: the value
// it carries is seen after the loop.
function ln_for_in_continue_edge(xs: int[]) -> string {
    let x: int | LnTag = 0;
    for let v in xs {
        if (v > 0) {
            x = LnTag { name: "tag" };
            continue;
        }
        x = v;
    }
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

test "the break edge's value is seen after the loop" {
    assert.equal(ln_break_edge_value(1), "tag");
    assert.equal(ln_break_edge_value(0), "int");
}

test "a break as the only exit keeps its write" {
    assert.equal(ln_break_only_exit(41), 42);
}

test "an is-test loop exits with the residual" {
    assert.equal(ln_is_test_exit(1, "s"), "s");
}

test "a for-in collection is read in the entry flow" {
    assert.equal(ln_collection_in_entry_flow(LnBag { items: [1, 2, 3] }), 6);
    assert.equal(ln_collection_in_entry_flow(null), 0);
}

test "the step slot sees the continue edge" {
    assert.equal(ln_step_after_continue(3), 1);
    assert.equal(ln_step_after_continue(0), 0);
}

test "a for-in continue on the last iteration is an exit edge" {
    assert.equal(ln_for_in_continue_edge([0, 5]), "tag");
    assert.equal(ln_for_in_continue_edge([5, 0]), "int");
    assert.equal(ln_for_in_continue_edge([]), "int");
}

// A header that always returns leaves the loop with no exit edge: the body's
// `break` is dead, and the function needs no tail.
function ln_header_always_returns() -> int {
    while ({ return 1; }) {
        break;
    }
}

// A `continue` in a loop nested inside the update slot binds to that loop.
function ln_nested_loop_in_the_update_slot() -> int {
    let i = 0;
    for (let j = 0; i < 3; i = { for let k in [1] { continue; } i + 1 }) {}
    i
}

// A collection that never produces a value: only the collection runs.
function ln_for_over_a_diverging_collection() -> int {
    for let v in ({ return 2; }) {}
}

// An always-truthy condition with a `break` in the body: the break is the exit.
function ln_unconditional_loop_exits_through_break(a: int) -> int {
    let n = 0;
    while (true) {
        n += 1;
        if (n > a) {
            break;
        }
    }
    n
}

// A destructuring pattern over such a collection binds nothing that exists.
class LnPair {
    left: int,
    right: int,
}

function ln_for_class_pattern_over_a_diverging_collection() -> int {
    for let LnPair { left: let l, right: let r } in ({ return 3; }) {}
}

function ln_for_tuple_pattern_over_a_diverging_collection() -> int {
    for let (let a, let b) in ({ return 4; }) {}
}

test "a loop with no exit edge ends the body; the update slot may hold a loop" {
    assert.equal(ln_header_always_returns(), 1);
    assert.equal(ln_nested_loop_in_the_update_slot(), 3);
    assert.equal(ln_for_over_a_diverging_collection(), 2);
    assert.equal(ln_for_class_pattern_over_a_diverging_collection(), 3);
    assert.equal(ln_for_tuple_pattern_over_a_diverging_collection(), 4);
    assert.equal(ln_unconditional_loop_exits_through_break(2), 3);
}
