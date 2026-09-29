// End-to-end semantics of tuples: `(a, b)` literals, `(A, B)` types, `t.0`
// element access, tuple patterns in `match` / `let`, equality, rendering, and
// JSON. Tuples are immutable, fixed-arity products; they are covariant
// elementwise and disjoint from lists.
//
// Every check runs through a helper function: a test-block local is boxed (VM
// quirk), so nothing is bound in a test block here. Names carry a `tp_` / `Tp`
// prefix to stay clear of the other namespaces in this one-project corpus.

// ─── Literals and element access ─────────────────────────────────────────────

function tp_pair() -> (int, string) {
    (1, "a")
}

function tp_first() -> int {
    tp_pair().0
}

function tp_second() -> string {
    tp_pair().1
}

test "tuple_element_access" {
    assert.equal(tp_first(), 1)
    assert.equal(tp_second(), "a")
}

function tp_single() -> (int,) {
    (7,)
}

// `(7)` is grouping, not a 1-tuple.
function tp_grouping() -> int {
    (7)
}

test "one_tuple_and_grouping" {
    assert.equal(tp_single().0, 7)
    assert.equal(tp_grouping(), 7)
}

// `t.0.1` lexes as `t` `.` `0.1`; the parser splits the float.
function tp_nested() -> string {
    let t = ((1, "inner"), true);
    t.0.1
}

test "nested_element_access" {
    assert.equal(tp_nested(), "inner")
}

// A tuple as a block's tail expression, on its own line after a `let` whose
// initializer ends in a call: it must not glue on as call arguments.
function tp_tail_after_call() -> (int, int) {
    let a = tp_first()
    (a, a)
}

test "tuple_tail_after_call" {
    assert.equal(tp_tail_after_call().1, 1)
}

// ─── Types: covariance, unions, containers ───────────────────────────────────

function tp_takes_optional(t: (int?, string)) -> bool {
    t.0 == null
}

// `(int, string)` is accepted where `(int?, string)` is expected: tuples are
// immutable, so they are covariant.
function tp_covariant() -> bool {
    let t: (int, string) = (1, "a");
    tp_takes_optional(t)
}

test "tuples_are_covariant" {
    assert.equal(tp_covariant(), false)
}

function tp_list_of_pairs() -> (string, int)[] {
    [("a", 1), ("b", 2), ("c", 3)]
}

function tp_sum_seconds() -> int {
    let total = 0;
    for (let p in tp_list_of_pairs()) {
        total = total + p.1;
    }
    total
}

test "list_of_tuples" {
    assert.equal(tp_sum_seconds(), 6)
}

function tp_map_of_pairs() -> map<string, (int, int)> {
    { "origin": (0, 0), "unit": (1, 1) }
}

test "map_of_tuples" {
    assert.equal(tp_map_of_pairs()["unit"].1, 1)
}

class TpSpan {
    range: (int, int)
    label: string
}

function tp_span_width() -> int {
    let s = TpSpan { range: (3, 10), label: "x" };
    s.range.1 - s.range.0
}

test "tuple_class_field" {
    assert.equal(tp_span_width(), 7)
}

// Reading slot 0 of a union of tuples reads that slot of whichever arm it is.
function tp_union_slot(flag: bool) -> int | string {
    let t: (int, bool) | (string, bool) = if (flag) { (1, true) } else { ("s", false) };
    t.0
}

test "union_of_tuples_slot_access" {
    assert.equal(tp_union_slot(true), 1)
    assert.equal(tp_union_slot(false), "s")
}

// ─── Generics ────────────────────────────────────────────────────────────────

function tp_swap<A, B>(t: (A, B)) -> (B, A) {
    (t.1, t.0)
}

function tp_identity<T>(x: T) -> T {
    x
}

test "generic_tuple_functions" {
    assert.equal(tp_swap((1, "a")).0, "a")
    assert.equal(tp_identity((1, 2)).1, 2)
}

// ─── Patterns ────────────────────────────────────────────────────────────────

function tp_destructure() -> int {
    let (let a, let b) = (3, 4);
    a * b
}

test "let_destructuring" {
    assert.equal(tp_destructure(), 12)
}

// `match (a, b)` scrutinizes a tuple; the four literal arms are exhaustive.
function tp_bool_cmp(a: bool, b: bool) -> int {
    match (a, b) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => -1,
        (false, false) => 0,
    }
}

test "match_on_tuple_scrutinee" {
    assert.equal(tp_bool_cmp(true, false), 1)
    assert.equal(tp_bool_cmp(false, true), -1)
    assert.equal(tp_bool_cmp(true, true), 0)
}

function tp_classify(t: (int, string)) -> string {
    match (t) {
        (0, _) => "zero",
        (let n, "x") => "x" + n.to_string(),
        (_, let s) => s,
    }
}

test "tuple_patterns_with_bindings" {
    assert.equal(tp_classify((0, "q")), "zero")
    assert.equal(tp_classify((5, "x")), "x5")
    assert.equal(tp_classify((5, "y")), "y")
}

// A tuple pattern against a union claims every same-arity tuple arm.
function tp_union_match(v: (int, string) | (string, int) | int) -> string {
    match (v) {
        (let a, let b) => "pair",
        let n: int => "int",
    }
}

test "tuple_pattern_over_union" {
    assert.equal(tp_union_match((1, "a")), "pair")
    assert.equal(tp_union_match(("a", 1)), "pair")
    assert.equal(tp_union_match(3), "int")
}

function tp_is_pair(v: unknown) -> bool {
    v is (int, string)
}

test "is_tuple_type" {
    assert.equal(tp_is_pair((1, "a")), true)
    assert.equal(tp_is_pair(("a", 1)), false)
    assert.equal(tp_is_pair([1, 2]), false)
    assert.equal(tp_is_pair((1, "a", true)), false)
}

// ─── Equality, rendering, JSON ───────────────────────────────────────────────

function tp_eq() -> bool {
    (1, "a") == (1, "a")
}

function tp_neq() -> bool {
    (1, "a") == (1, "b")
}

function tp_nested_eq() -> bool {
    ((1, [2, 3]), "x") == ((1, [2, 3]), "x")
}

test "tuple_equality_is_structural" {
    assert.equal(tp_eq(), true)
    assert.equal(tp_neq(), false)
    assert.equal(tp_nested_eq(), true)
}

function tp_render() -> string {
    string.from((1, "a", [true]))
}

function tp_render_single() -> string {
    string.from((1,))
}

test "tuple_rendering" {
    assert.equal(tp_render(), "(1, \"a\", [true])")
    assert.equal(tp_render_single(), "(1,)")
}

function tp_to_json() -> string {
    baml.json.stringify(baml.json.to_json((1, "a")))
}

function tp_from_json() -> (int, string) {
    baml.json.to<(int, string)>(baml.json.parse(`[2, "b"]`))
}

test "tuple_json_round_trip" {
    assert.equal(tp_to_json(), "[1,\"a\"]")
    assert.equal(tp_from_json().1, "b")
}

// ─── Reflection ──────────────────────────────────────────────────────────────

function tp_reflect_arity() -> int {
    match (reflect.Type.of<(int, string)>().as_tuple()) {
        null => -1,
        let view: reflect.tuple.Type => view.element_types().length(),
    }
}

function tp_reflect_value_kind() -> bool {
    reflect.Type.of_value((1, "a")).as_tuple() != null
}

test "tuple_reflection" {
    assert.equal(tp_reflect_arity(), 2)
    assert.equal(tp_reflect_value_kind(), true)
}
