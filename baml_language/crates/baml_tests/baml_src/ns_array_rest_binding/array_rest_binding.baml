// Runtime semantics of array rest-pattern bindings (B-531): `..let r`
// binds a copy of the unmatched middle of the scrutinee, typed `elem[]`.
//
// Tests use top-level helper functions (`arb_*`) with `match` / locals because
// these must live in a function to avoid the test-block local-boxing VM bug.
// Classes are prefixed (`ArbCell`, `ArbNumberBag`) to keep names unique across
// the shared corpus project namespace.

// ─── basic rest binding in match arms ────────────────────────────────────────

function arb_rest_binds_middle_values() -> int {
    match ([1, 2, 3, 4, 5]) {
        [let a, ..let r, let z] => r[0] * 100 + r[1] * 10 + r[2] + a * 1000 + z * 10000,
        _ => -1
    }
}

test "rest_binds_middle_values" {
    // a=1, r=[2,3,4], z=5
    assert.equal(arb_rest_binds_middle_values(), 51234)
}

function arb_rest_with_no_suffix_takes_tail() -> int {
    match ([7, 8, 9]) {
        [let a, ..let r] => r.length() * 10 + r[0] - a,
        _ => -1
    }
}

test "rest_with_no_suffix_takes_tail" {
    // r=[8,9]: 2*10 + 8 - 7
    assert.equal(arb_rest_with_no_suffix_takes_tail(), 21)
}

function arb_rest_is_empty_when_nothing_remains() -> int {
    match ([1]) {
        [let a, ..let r] => r.length(),
        _ => -1
    }
}

test "rest_is_empty_when_nothing_remains" {
    assert.equal(arb_rest_is_empty_when_nothing_remains(), 0)
}

function arb_rest_is_empty_between_exact_prefix_and_suffix() -> int {
    match ([1, 2]) {
        [let a, ..let r, let z] => r.length() * 100 + a * 10 + z,
        _ => -1
    }
}

test "rest_is_empty_between_exact_prefix_and_suffix" {
    // r=[], a=1, z=2
    assert.equal(arb_rest_is_empty_between_exact_prefix_and_suffix(), 12)
}

function arb_too_short_array_falls_through_rest_arm() -> int {
    match ([9]) {
        [let a, ..let r, let z] => 1,
        _ => 2
    }
}

test "too_short_array_falls_through_rest_arm" {
    assert.equal(arb_too_short_array_falls_through_rest_arm(), 2)
}

function arb_rest_only_copies_whole_array() -> int {
    let [..let all] = [1, 2, 3]
    return all.length() * 100 + all[0] * 10 + all[2]
}

test "rest_only_copies_whole_array" {
    assert.equal(arb_rest_only_copies_whole_array(), 313)
}

// ─── the rest binding is a copy, not a view ──────────────────────────────────

function arb_pushing_to_rest_does_not_mutate_source() -> int {
    let xs = [1, 2, 3];
    match (xs) {
        [let a, ..let r] => {
            r.push(99);
            xs.length() * 10 + r.length()
        },
        _ => -1
    }
}

test "pushing_to_rest_does_not_mutate_source" {
    // xs stays [1,2,3]; r becomes [2,3,99]
    assert.equal(arb_pushing_to_rest_does_not_mutate_source(), 33)
}

function arb_pushing_to_source_does_not_mutate_rest() -> int {
    let xs = [1, 2, 3];
    match (xs) {
        [let a, ..let r] => {
            xs.push(7);
            r.length()
        },
        _ => -1
    }
}

test "pushing_to_source_does_not_mutate_rest" {
    assert.equal(arb_pushing_to_source_does_not_mutate_rest(), 2)
}

class ArbCell {
    v int
}

function arb_rest_copy_is_shallow_elements_are_shared() -> int {
    let c = ArbCell { v: 1 };
    let xs = [ArbCell { v: 2 }, c];
    match (xs) {
        [let first, ..let r] => {
            c.v = 5;
            r[0].v
        },
        _ => -1
    }
}

test "rest_copy_is_shallow_elements_are_shared" {
    // r=[c]; mutating c is visible through the copied slice.
    assert.equal(arb_rest_copy_is_shallow_elements_are_shared(), 5)
}

// ─── or-arm rest bindings across a union scrutinee ───────────────────────────

class ArbNumberBag {
    field int[]
}

function arb_pick(v: ArbNumberBag | int[][]) -> int {
    match (v) {
        ArbNumberBag { field } | [[..let field]: int[], .._] => field[0],
        _ => 0
    }
}

function arb_class_branch() -> int {
    arb_pick(ArbNumberBag { field: [7, 8] })
}

function arb_array_branch() -> int {
    arb_pick([[5, 6], [9]])
}

function arb_empty_branch() -> int {
    let empty: int[][] = []
    arb_pick(empty)
}

test "or_arm_rest_binding_dispatches_class_branch" {
    assert.equal(arb_class_branch(), 7)
}

test "or_arm_rest_binding_dispatches_array_branch" {
    assert.equal(arb_array_branch(), 5)
}

test "or_arm_rest_binding_falls_through_empty_array" {
    assert.equal(arb_empty_branch(), 0)
}

// ─── rest bindings in other binding positions ────────────────────────────────

function arb_rest_binding_in_nested_element_pattern() -> int {
    match ([[1, 2], [3]]) {
        [[..let inner], .._] => inner.length() * 10 + inner[1],
        _ => -1
    }
}

test "rest_binding_in_nested_element_pattern" {
    // inner=[1,2]
    assert.equal(arb_rest_binding_in_nested_element_pattern(), 22)
}

function arb_grab(xs: int[]) -> int {
    if let [let a, ..let r] = xs {
        return a * 10 + r.length();
    }
    return -1;
}

function arb_rest_binding_in_if_let() -> int {
    arb_grab([4, 5, 6]) * 100 + arb_grab([])
}

test "rest_binding_in_if_let" {
    // grab([4,5,6]) = 42; grab([]) = -1
    assert.equal(arb_rest_binding_in_if_let(), 4199)
}

function arb_rest_only_binding_in_for_loop() -> int {
    let empty: int[] = [];
    let rows = [[1, 2, 3], [4], empty];
    let total = 0;
    for (let [..let r] in rows) {
        total += r.length();
    }
    total
}

test "rest_only_binding_in_for_loop" {
    assert.equal(arb_rest_only_binding_in_for_loop(), 4)
}

function arb_tail_len<T>(xs: T[]) -> int {
    match (xs) {
        [let first, ..let r] => r.length(),
        _ => -1
    }
}

function arb_rest_binding_generic_element_type() -> int {
    arb_tail_len(["a", "b", "c"]) * 10 + arb_tail_len([true])
}

test "rest_binding_generic_element_type" {
    // 2*10 + 0
    assert.equal(arb_rest_binding_generic_element_type(), 20)
}

function arb_rest_bind_chain_aliases_same_slice() -> int {
    match ([1, 2, 3]) {
        [..let r: let s] => {
            r.push(4);
            r.length() * 10 + s.length()
        }
    }
}

test "rest_bind_chain_aliases_same_slice" {
    // Mutating one chained binding is observed through the other, proving `r`
    // and `s` reference the same slice rather than two equal-length copies.
    assert.equal(arb_rest_bind_chain_aliases_same_slice(), 44)
}
