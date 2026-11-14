

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

function tp_grouping() -> int {
    (7)
}

test "one_tuple_and_grouping" {
    assert.equal(tp_single().0, 7)
    assert.equal(tp_grouping(), 7)
}

function tp_nested() -> string {
    let t = ((1, "inner"), true);
    t.0.1
}

test "nested_element_access" {
    assert.equal(tp_nested(), "inner")
}

function tp_tail_after_call() -> (int, int) {
    let a = tp_first()
    (a, a)
}

test "tuple_tail_after_call" {
    assert.equal(tp_tail_after_call().1, 1)
}


function tp_takes_optional(t: (int?, string)) -> bool {
    t.0 == null
}

function tp_fresh_literal_at_wider_slots() -> bool {
    tp_takes_optional((1, "a"))
}

function tp_rebuilt_at_wider_slots() -> bool {
    let t: (int, string) = (1, "a");
    tp_takes_optional((t.0, t.1))
}

function tp_fresh_literal_carries_expected() -> bool {
    let t: (int?, string) = (1, "a");
    t is (int?, string)
}

test "tuples_are_invariant" {
    assert.equal(tp_fresh_literal_at_wider_slots(), false)
    assert.equal(tp_rebuilt_at_wider_slots(), false)
    assert.is_true(tp_fresh_literal_carries_expected())
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

function tp_union_slot(flag: bool) -> int | string {
    let t: (int, bool) | (string, bool) = if (flag) { (1, true) } else { ("s", false) };
    match t { (let first, _) => first }
}

test "union_of_tuples_slot_access" {
    assert.equal(tp_union_slot(true), 1)
    assert.equal(tp_union_slot(false), "s")
}


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


function tp_destructure() -> int {
    let (let a, let b) = (3, 4);
    a * b
}

test "let_destructuring" {
    assert.equal(tp_destructure(), 12)
}

function tp_bool_cmp(a: bool, b: bool) -> int {
    match (a, b) {
        (true, let _x: true) => 0,
        (true, let _x: false) => 1,
        (false, let _x: true) => -1,
        (false, let _x: false) => 0,
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
    baml.json.stringify((1, "a"))
}

function tp_from_value() -> string {
    baml.json.stringify(baml.json.from((1, [true])))
}

function tp_decode_text() -> (int, string) {
    baml.json.decode<(int, string)>(`[2, "b"]`)
}

function tp_decode_tree() -> (int, string) {
    baml.json.to<(int, string)>(baml.json.parse(`[3, "c"]`))
}

function tp_decode_by_arity() -> int {
    match (baml.json.decode<(int,) | (int, int)>(`[4, 5]`)) {
        (let a,) => a,
        (let a, let b) => a + b,
    }
}

function tp_decode_wrong_arity() -> bool {
    baml.json.decode<(int, string)>(`[1]`) catch (e) {
        baml.json.DecodeError => return true,
    };
    false
}

test "tuple_json_round_trip" {
    assert.equal(tp_to_json(), "[1,\"a\"]")
    assert.equal(tp_from_value(), "[1,[true]]")
    assert.equal(tp_decode_text().1, "b")
    assert.equal(tp_decode_tree().1, "c")
    assert.equal(tp_decode_by_arity(), 9)
    assert.is_true(tp_decode_wrong_arity())
}


function tp_reflect_arity() -> int {
    match (reflect.Type.of<(int, string)>().as_tuple()) {
        null => -1,
        let view: reflect.Tuple => view.element_types().length(),
    }
}

function tp_reflect_value_kind() -> bool {
    reflect.Type.of_value((1, "a")).as_tuple() != null
}

test "tuple_reflection" {
    assert.equal(tp_reflect_arity(), 2)
    assert.equal(tp_reflect_value_kind(), true)
}


function tp_sap_pair(raw: string) -> (int, string) {
    baml.sap.parse<(int, string)>(raw)
}

function tp_sap_pairs_inhabit(raw: string) -> bool {
    let parsed: unknown = baml.sap.parse<(int, string)[]>(raw);
    match (parsed) {
        let _pairs: (int, string)[] => true,
        _ => false,
    }
}

test "tuple_sap_parse" {
    let pair = tp_sap_pair(`[1, "a"]`);
    assert.equal(pair.0, 1)
    assert.equal(pair.1, "a")
    assert.equal(tp_sap_pair(`[2, "b", "extra"]`).1, "b")
    assert.is_true(tp_sap_pairs_inhabit(`[[1, "a"], [2, "b"]]`))
}

test "tuple_sap_missing_element_throws" {
    {
        tp_sap_pair(`[1]`);
        baml.sys.panic("expected a tuple missing an element to fail to parse")
    } catch (e) {
        baml.errors.ParseError => null
    }
}


function tp_bind_whole() -> int {
    let whole: (let first, _) = (1, 2);
    first * 10 + whole.1
}

test "tuple_destructure_beneath_a_binding" {
    assert.equal(tp_bind_whole(), 12)
}


class TpBox {
    x: int,
}

function tp_optional_index(t: (int, string)?) -> string? {
    t?.1
}

function tp_assign_through_element() -> int {
    let t = (TpBox { x: 1 }, 2);
    t.0.x = 5;
    t.0.x
}

function tp_union_index(t: (int, string) | (string, string)) -> string {
    match t { (let _, let label) => label }
}

test "tuple_indexing" {
    assert.equal(tp_optional_index((1, "a")), "a")
    assert.equal(tp_optional_index(null), null)
    assert.equal(tp_assign_through_element(), 5)
    assert.equal(tp_union_index(("x", "y")), "y")
}


function tp_is_of<T>(v: unknown) -> bool {
    v is T
}

function tp_wrap<T>(x: T) -> (T,) {
    (x,)
}

function tp_wrap_optional_is() -> bool {
    tp_wrap<int?>(1) is (int?,)
}

function tp_wrap_optional_is_not_int() -> bool {
    let w: unknown = tp_wrap<int?>(1);
    w is (int,)
}

function tp_wrap_optional_element_is_int() -> bool {
    tp_wrap<int?>(1) is (let _x: int,)
}

function tp_wrap_optional_is_int() -> bool {
    tp_is_of<(int,)>(tp_wrap<int?>(1))
}

function tp_wrap_optional_type() -> bool {
    reflect.Type.of_value(tp_wrap<int?>(1)) == reflect.Type.of<(int?,)>()
}

function tp_wrap_int_type() -> bool {
    reflect.Type.of_value(tp_wrap<int>(1)) == reflect.Type.of<(int,)>()
}

test "generic_construction_carries_the_realized_type_argument" {
    assert.is_true(tp_wrap_optional_is())
    assert.is_true(tp_is_of<(int?,)>(tp_wrap<int?>(1)))
    assert.equal(tp_wrap_optional_is_int(), false)
    assert.equal(tp_wrap_optional_is_not_int(), false)
    assert.is_true(tp_wrap_optional_element_is_int())
    assert.is_true(tp_wrap_optional_type())
    assert.is_true(tp_wrap_int_type())
}

function tp_broad() -> unknown {
    let broad: (string,) = ("foo",);
    broad
}

function tp_exact() -> unknown {
    let exact: ("foo",) = ("foo",);
    exact
}

function tp_literal_slots() -> string {
    let broad = tp_broad();
    let exact = tp_exact();
    tp_is_of<("foo",)>(broad).to_string()
        + " " + tp_is_of<("foo",)>(exact).to_string()
        + " " + tp_is_of<(string,)>(broad).to_string()
        + " " + tp_is_of<(string,)>(exact).to_string()
}

function tp_literal_slot_types() -> bool {
    reflect.Type.of_value(tp_broad()) == reflect.Type.of<(string,)>()
        && reflect.Type.of_value(tp_exact()) == reflect.Type.of<("foo",)>()
}

test "a_literal_slot_is_carried_only_when_it_is_the_slot_type" {
    assert.equal(tp_literal_slots(), "false true true false")
    assert.is_true(tp_literal_slot_types())
}

function tp_uncontextual_type() -> bool {
    reflect.Type.of_value((1, "a")) == reflect.Type.of<(int, string)>()
}

test "an_uncontextual_tuple_carries_widened_slot_types" {
    assert.is_true(tp_uncontextual_type())
}

function tp_union_slot_carried() -> bool {
    let t: (int | string, bool) = (1, true);
    let held: unknown = t;
    reflect.Type.of_value(held) == reflect.Type.of<(int | string, bool)>()
        && !tp_is_of<(int, bool)>(held)
}

test "a_union_slot_is_carried" {
    assert.is_true(tp_union_slot_carried())
}

function tp_equal_across_slot_types() -> bool {
    tp_broad() == tp_exact()
}

test "equality_ignores_carried_slot_types" {
    assert.is_true(tp_equal_across_slot_types())
}

function tp_deep_copy_type() -> bool {
    let t: (int?, string[]) = (1, ["a"]);
    let copied: unknown = baml.deep_copy(t);
    reflect.Type.of_value(copied) == reflect.Type.of<(int?, string[])>()
}

test "deep_copy_keeps_carried_slot_types" {
    assert.is_true(tp_deep_copy_type())
}

function tp_decoded_slot_type() -> bool {
    let decoded: unknown = baml.json.decode<("a" | "b", int?)>(`["a", 1]`);
    reflect.Type.of_value(decoded) == reflect.Type.of<("a" | "b", int?)>()
        && !tp_is_of<("a", int)>(decoded)
}

test "decoded_tuple_carries_its_target_slot_types" {
    assert.is_true(tp_decoded_slot_type())
}

function tp_swap_by_pattern<A, B>(t: (A, B)) -> (B, A) {
    let (let a, let b) = t;
    (b, a)
}

function tp_swap_by_pattern_type() -> bool {
    let swapped: unknown = tp_swap_by_pattern<int, string?>((1, "a"));
    reflect.Type.of_value(swapped) == reflect.Type.of<(string?, int)>()
}

test "tuple_pattern_projection_and_generic_rebuild" {
    assert.is_true(tp_swap_by_pattern_type())
}
