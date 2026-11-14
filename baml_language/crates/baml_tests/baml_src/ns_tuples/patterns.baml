// Tuple patterns in conditional positions, and the annotated scrutinee form,
// which needs its own outer parentheses: `match ((a, b): T)`.

function tpp_annotated_scrutinee(a: int, b: string?) -> string {
    match ((a, b): (int, string?)) {
        // A binding makes this a destructure: `(0, null)` alone would test
        // for the tuple type `(0, null)`, which no `(int, string?)` carries.
        (0, let _s: null) => "zero",
        (let n, null) => `${n}`,
        (_, let s: string) => s,
    }
}

test "tuple_match_annotated_scrutinee" {
    assert.equal(tpp_annotated_scrutinee(0, null), "zero")
    assert.equal(tpp_annotated_scrutinee(3, null), "3")
    assert.equal(tpp_annotated_scrutinee(3, "s"), "s")
}

function tpp_if_let(pair: (int, string)?) -> string {
    if let (let n, let s) = pair {
        `${s}${n}`
    } else {
        "none"
    }
}

// A refutable element only matches its value.
function tpp_if_let_refutable(pair: (int, string)) -> string {
    if let (0, let s) = pair {
        s
    } else {
        "nonzero"
    }
}

function tpp_if_const(pair: (int, int)?) -> int {
    if const (const a, const b) = pair {
        a + b
    } else {
        -1
    }
}

test "tuple_if_let" {
    assert.equal(tpp_if_let((1, "a")), "a1")
    assert.equal(tpp_if_let(null), "none")
    assert.equal(tpp_if_let_refutable((0, "zero")), "zero")
    assert.equal(tpp_if_let_refutable((1, "one")), "nonzero")
    assert.equal(tpp_if_const((2, 3)), 5)
    assert.equal(tpp_if_const(null), -1)
}

// A binding whose subpattern tests a value binds the value that test
// examined, never a later read: a guard that writes the field between the
// test and the arm cannot make the binding disagree with its type. Tuple
// patterns share this path; a tuple twin of this test needs slot writes.
class TppBox {
    v: int?,
}

function tpp_clear(b: TppBox) -> bool {
    b.v = null;
    true
}

function tpp_class_bind_from_tested(b: TppBox) -> int? {
    match (b) {
        TppBox { v: let n: int } if (tpp_clear(b)) => n,
        _ => -1,
    }
}

test "pattern_binds_the_tested_value" {
    assert.equal(tpp_class_bind_from_tested(TppBox { v: 7 }), 7)
}

// A tuple pattern on `unknown` tests the shape at runtime (a tuple of this
// arity, whatever its carried slot types), and type-tests elements by value.
function tpp_unknown_shape(v: unknown) -> string {
    match (v) {
        (let a: int, let b: string) => `${b}${a}`,
        (let a, let b) => "pair",
        (let a,) => "single",
        _ => "other",
    }
}

test "tuple_shape_test_ignores_carried_slot_types" {
    assert.equal(tpp_unknown_shape((1, "a")), "a1")
    assert.equal(tpp_unknown_shape(("a", 1)), "pair")
    assert.equal(tpp_unknown_shape((true,)), "single")
    assert.equal(tpp_unknown_shape([1, 2]), "other")
    assert.equal(tpp_unknown_shape(3), "other")
}

// An element test removes a union arm whose slot type it is disjoint from;
// the remaining arm is kept whole, and the residual is the other arm.
function tpp_takes_int_row(t: (int, bool)) -> int {
    if (t.1) { t.0 } else { -t.0 }
}

function tpp_arm_elimination(t: (int, bool) | (string, bool)) -> int {
    match (t) {
        (let n: int, _) => tpp_takes_int_row(t),
        (let s: string, _) => s.length(),
    }
}

// Whole-tuple type tests compare carried types: a `(int?, bool)` holding an
// int is not an `(int, bool)`.
function tpp_carried_arm(t: (int?, bool) | (int, bool)) -> string {
    match (t) {
        let r: (int, bool) => "narrow",
        let r: (int?, bool) => "wide",
    }
}

function tpp_wide_row() -> (int?, bool) {
    (1, true)
}

test "tuple_arm_selection" {
    assert.equal(tpp_arm_elimination((2, false)), -2)
    assert.equal(tpp_arm_elimination(("abc", true)), 3)
    assert.equal(tpp_carried_arm((1, true)), "narrow")
    assert.equal(tpp_carried_arm(tpp_wide_row()), "wide")
}
