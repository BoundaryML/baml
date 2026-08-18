// Truthiness in condition positions (B-1563).
//
// `if`/`while`/match guards and `&&`/`||`/`!` operands accept any value.
// Falsy: `false`, `null`, `0`, `0n`, `0.0`, `""`, `[]`, `{}`, empty bytes.
// Everything else is truthy, including NaN, instances, variants, and
// closures. The narrowing rule drops always-falsy union members in the
// true branch and always-truthy members in the false branch, so a
// `string?` condition interpolates as `string` in the then-branch.
//
// Helpers take parameters so the always-truthy/always-falsy warning
// (E0164) never fires in this corpus: parameter types stay runtime-decided.

function tr_pick(v: bool) -> string {
    if (v) { "T" } else { "F" }
}

// ============================================================================
// 1. Falsy values
// ============================================================================

function tr_int(v: int) -> string {
    if (v) { "T" } else { "F" }
}

test "int_zero_falsy" { assert.equal(tr_int(0), "F") }
test "int_nonzero_truthy" { assert.equal(tr_int(7), "T") }
test "int_negative_truthy" { assert.equal(tr_int(-1), "T") }

function tr_float(v: float) -> string {
    if (v) { "T" } else { "F" }
}

test "float_zero_falsy" { assert.equal(tr_float(0.0), "F") }
test "float_neg_zero_falsy" { assert.equal(tr_float(-0.0), "F") }
test "float_nonzero_truthy" { assert.equal(tr_float(0.5), "T") }

function tr_float_nan() -> string {
    tr_float(float.nan())
}

test "float_nan_truthy" { assert.equal(tr_float_nan(), "T") }

function tr_bigint(v: bigint) -> string {
    if (v) { "T" } else { "F" }
}

test "bigint_zero_falsy" { assert.equal(tr_bigint(0n), "F") }
test "bigint_nonzero_truthy" { assert.equal(tr_bigint(5n), "T") }

function tr_string(v: string) -> string {
    if (v) { "T" } else { "F" }
}

test "string_empty_falsy" { assert.equal(tr_string(""), "F") }
test "string_nonempty_truthy" { assert.equal(tr_string("x"), "T") }
test "string_space_truthy" { assert.equal(tr_string(" "), "T") }

function tr_list(v: int[]) -> string {
    if (v) { "T" } else { "F" }
}

test "list_empty_falsy" { assert.equal(tr_list([]), "F") }
test "list_nonempty_truthy" { assert.equal(tr_list([0]), "T") }

function tr_map(v: map<string, int>) -> string {
    if (v) { "T" } else { "F" }
}

function tr_run_map_empty() -> string {
    let m: map<string, int> = {};
    tr_map(m)
}

function tr_run_map_nonempty() -> string {
    tr_map({ "a": 0 })
}

test "map_empty_falsy" { assert.equal(tr_run_map_empty(), "F") }
test "map_nonempty_truthy" { assert.equal(tr_run_map_nonempty(), "T") }

function tr_bytes(v: uint8array) -> string {
    if (v) { "T" } else { "F" }
}

test "bytes_empty_falsy" { assert.equal(tr_bytes(b""), "F") }
test "bytes_nonempty_truthy" { assert.equal(tr_bytes(b"\x00"), "T") }

function tr_opt_bool(v: bool?) -> string {
    if (v) { "T" } else { "F" }
}

test "bool_false_falsy" { assert.equal(tr_opt_bool(false), "F") }
test "bool_true_truthy" { assert.equal(tr_opt_bool(true), "T") }
test "bool_null_falsy" { assert.equal(tr_opt_bool(null), "F") }

// ============================================================================
// 2. Truthy heap values
// ============================================================================

class TrPoint { x int }

enum TrColor { Red, Green }

function tr_instance(v: TrPoint?) -> string {
    if (v) { "T" } else { "F" }
}

test "instance_truthy" { assert.equal(tr_instance(TrPoint { x: 0 }), "T") }
test "instance_null_falsy" { assert.equal(tr_instance(null), "F") }

function tr_variant(v: TrColor?) -> string {
    if (v) { "T" } else { "F" }
}

test "variant_truthy" { assert.equal(tr_variant(TrColor.Red), "T") }

function tr_closure(v: ((int) -> int throws never)?) -> string {
    if (v) { "T" } else { "F" }
}

function tr_run_closure() -> string {
    tr_closure((n: int) -> int { n })
}

test "closure_truthy" { assert.equal(tr_run_closure(), "T") }

// ============================================================================
// 3. Narrowing: string? condition interpolates as string
// ============================================================================

function tr_frag(instructions: string?) -> string {
    if (instructions) {
        `<x>${instructions}</x>`
    } else {
        ""
    }
}

test "narrow_some" { assert.equal(tr_frag("hi"), "<x>hi</x>") }
test "narrow_null" { assert.equal(tr_frag(null), "") }
test "narrow_empty" { assert.equal(tr_frag(""), "") }

function tr_frag_not(instructions: string?) -> string {
    if (!instructions) {
        ""
    } else {
        `<x>${instructions}</x>`
    }
}

test "narrow_not_some" { assert.equal(tr_frag_not("hi"), "<x>hi</x>") }
test "narrow_not_null" { assert.equal(tr_frag_not(null), "") }

// ============================================================================
// 4. Logical operators coerce operands; the result stays bool
// ============================================================================

function tr_and(a: string?, b: int) -> bool {
    a && b
}

test "and_both_truthy" { assert.is_true(tr_and("x", 1)) }
test "and_lhs_falsy" { assert.is_true(!(tr_and(null, 1))) }
test "and_rhs_falsy" { assert.is_true(!(tr_and("x", 0))) }
test "and_empty_lhs_falsy" { assert.is_true(!(tr_and("", 1))) }

function tr_or(a: string?, b: int) -> bool {
    a || b
}

test "or_lhs_truthy" { assert.is_true(tr_or("x", 0)) }
test "or_rhs_truthy" { assert.is_true(tr_or(null, 2)) }
test "or_both_falsy" { assert.is_true(!(tr_or("", 0))) }

function tr_not(v: string?) -> bool {
    !v
}

test "not_null_true" { assert.is_true(tr_not(null)) }
test "not_empty_true" { assert.is_true(tr_not("")) }
test "not_value_false" { assert.is_true(!(tr_not("x"))) }

// ============================================================================
// 5. while and match guards
// ============================================================================

function tr_countdown(n: int) -> int {
    let steps = 0;
    let remaining = n;
    while (remaining) {
        remaining = remaining - 1;
        steps = steps + 1;
    }
    steps
}

test "while_int_counts_to_zero" { assert.equal(tr_countdown(3), 3) }
test "while_zero_skips" { assert.equal(tr_countdown(0), 0) }

function tr_guard(v: int, label: string?) -> string {
    match v {
        _ if label => `${v}-labeled`,
        _ => `${v}-plain`,
    }
}

test "guard_truthy" { assert.equal(tr_guard(1, "x"), "1-labeled") }
test "guard_null_falsy" { assert.equal(tr_guard(2, null), "2-plain") }
test "guard_empty_falsy" { assert.equal(tr_guard(3, ""), "3-plain") }

// ============================================================================
// 6. Deferred decisions: conditions typed by an inference variable
// ============================================================================

function tr_id<T>(v: T) -> T { v }

function tr_generic_conditions() -> string {
    let r = "";
    if (tr_id(0)) { r = r + "i0-T;" } else { r = r + "i0-F;" }
    if (tr_id("")) { r = r + "s-T;" } else { r = r + "s-F;" }
    if (tr_id("x")) { r = r + "x-T;" } else { r = r + "x-F;" }
    if (!tr_id(0)) { r = r + "n0-T;" } else { r = r + "n0-F;" }
    r
}

test "generic_call_conditions_coerce" {
    assert.equal(tr_generic_conditions(), "i0-F;s-F;x-T;n0-T;")
}

// ============================================================================
// 7. Literal-union narrowing polarity
// ============================================================================

// The true branch drops the always-falsy literal members ("" and null),
// so interpolation sees a plain string; the false branch drops the
// always-truthy member ("x").
function tr_literal_union(v: "" | "x" | null) -> string {
    if (v) {
        `<${v}>`
    } else {
        "none"
    }
}

test "literal_union_truthy_member" { assert.equal(tr_literal_union("x"), "<x>") }
test "literal_union_empty_falsy" { assert.equal(tr_literal_union(""), "none") }
test "literal_union_null_falsy" { assert.equal(tr_literal_union(null), "none") }
