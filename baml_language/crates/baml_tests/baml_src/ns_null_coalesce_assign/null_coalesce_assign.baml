// Regression: `x = <expr> ?? x` must keep `x` when the left operand is null.
//
// INVARIANT: `??` writes its destination on exactly one branch, after that
// branch's value is known. The destination of an assignment statement is the
// assignment target itself, so a destination written *before* the right
// operand runs would be read back clobbered by any right operand that mentions
// the target (`a = n ?? a`, `s.a = n ?? s.a`).

class NcaBox {
    a: int?,
    b: int?,
}

// ─── local self-assignment ──────────────────────────────────────────────────

function nca_local_int(seed: int?, incoming: int?) -> int? {
    let a = seed;
    a = incoming ?? a;
    a
}

function nca_local_string(seed: string?, incoming: string?) -> string? {
    let a = seed;
    a = incoming ?? a;
    a
}

// ─── field self-assignment ──────────────────────────────────────────────────

function nca_field_self(seed: int?, incoming: int?) -> int? {
    let s = NcaBox { a: seed, b: 99 };
    s.a = incoming ?? s.a;
    s.a
}

// ─── cross-field: never broken, must stay working ───────────────────────────

function nca_cross_field(a: int?, b: int?) -> int? {
    let s = NcaBox { a: a, b: b };
    s.a = s.a ?? s.b;
    s.a
}

// ─── nested coalesce with the target as innermost fallback ──────────────────

function nca_nested(seed: int?, outer: int?, inner: int?) -> int? {
    let a = seed;
    a = outer ?? (inner ?? a);
    a
}

// ─── method-call left operand ───────────────────────────────────────────────

function nca_method_lhs(seed: int?, xs: int[]) -> int? {
    let a = seed;
    a = xs.at(0) ?? a;
    a
}

// ─── via-temp equivalence: the spelling that always worked ──────────────────

function nca_via_temp(seed: int?, incoming: int?) -> int? {
    let a = seed;
    let next = incoming ?? a;
    a = next;
    a
}

// ─── ordinary (non-aliasing) `??` must be unaffected ────────────────────────

function nca_plain(n: int?, fallback: int) -> int {
    n ?? fallback
}

function nca_plain_chain(x: int?, y: int?, z: int) -> int {
    x ?? y ?? z
}

// ─── the right operand must run only when the left is null ──────────────────

function nca_rhs_not_evaluated(seed: int?, incoming: int?) -> int {
    let calls = 0;
    let a = seed;
    a = incoming ?? {
        calls = calls + 1;
        a
    };
    calls
}

test "local self-assignment keeps the previous value" {
    assert.equal(nca_local_int(3, null), 3);
    assert.equal(nca_local_int(3, 7), 7);
    assert.equal(nca_local_int(null, null), null);
    assert.equal(nca_local_string("hi", null), "hi");
    assert.equal(nca_local_string("hi", "bye"), "bye")
}

test "field self-assignment keeps the previous value" {
    assert.equal(nca_field_self(3, null), 3);
    assert.equal(nca_field_self(3, 7), 7);
    assert.equal(nca_field_self(null, null), null)
}

test "cross-field coalesce still works" {
    assert.equal(nca_cross_field(null, 5), 5);
    assert.equal(nca_cross_field(1, 5), 1);
    assert.equal(nca_cross_field(null, null), null)
}

test "nested coalesce keeps the target as innermost fallback" {
    assert.equal(nca_nested(7, null, null), 7);
    assert.equal(nca_nested(7, null, 2), 2);
    assert.equal(nca_nested(7, 1, 2), 1)
}

test "method-call left operand" {
    assert.equal(nca_method_lhs(9, []), 9);
    assert.equal(nca_method_lhs(9, [4]), 4)
}

test "via-temp spelling agrees with direct self-assignment" {
    assert.equal(nca_via_temp(3, null), nca_local_int(3, null));
    assert.equal(nca_via_temp(3, 7), nca_local_int(3, 7));
    assert.equal(nca_via_temp(null, null), nca_local_int(null, null))
}

test "plain coalesce is unaffected" {
    assert.equal(nca_plain(null, 4), 4);
    assert.equal(nca_plain(1, 4), 1);
    assert.equal(nca_plain_chain(null, null, 3), 3);
    assert.equal(nca_plain_chain(null, 2, 3), 2);
    assert.equal(nca_plain_chain(1, 2, 3), 1)
}

test "right operand runs only when the left is null" {
    assert.equal(nca_rhs_not_evaluated(3, 7), 0);
    assert.equal(nca_rhs_not_evaluated(3, null), 1)
}
