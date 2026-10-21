// End-to-end tests for the broad `==` driver `baml.ops.equals_equals` (the
// may-yield `$rust_function` worklist in `package_baml::ops`), plus the sibling
// comparison surface `baml.ops.Compare` — reached both by method call and, since
// ordering operators gained an interface lowering, by `<` `<=` `>` `>=`.
//
// Converted from crates/bex_vm/tests/comparison_driver.rs, where every case
// compiled a source string and ran a no-arg `bool` function through the real VM.
// Those entry points survive as top-level helpers, so each assertion is
// `assert.is_true(<helper>())`: a test-block local is boxed (VM quirk), so
// nothing is bound in a test block here.
//
// The corpus is one project, so every class, enum and function carries a
// `Cd` / `cd_` prefix to stay clear of the other namespaces.

// ─── Shared prelude: primitives, enum identity, classes, containers ──────────
// Class instances are built through helper functions, mirroring the Rust
// prelude, and each `cd_eq_*` entry point returns one bool.

enum CdColor {
    Red
    Green
}

class CdPoint {
    x: int
    y: int
}

class CdLine {
    start: CdPoint
    end: CdPoint
}

function cd_mk_point(x: int, y: int) -> CdPoint {
    CdPoint { x: x, y: y }
}

function cd_mk_line(ax: int, ay: int, bx: int, by: int) -> CdLine {
    CdLine { start: cd_mk_point(ax, ay), end: cd_mk_point(bx, by) }
}

function cd_eq_int_same() -> bool { baml.ops.equals_equals(1, 1) }
function cd_eq_int_diff() -> bool { baml.ops.equals_equals(1, 2) }
function cd_eq_string_same() -> bool { baml.ops.equals_equals("hi", "hi") }
function cd_eq_string_diff() -> bool { baml.ops.equals_equals("hi", "ho") }
function cd_eq_bool() -> bool { baml.ops.equals_equals(true, true) }

// Concrete-type gate: different runtime types are never equal (never an error).
function cd_eq_cross_type() -> bool { baml.ops.equals_equals(1, "1") }
function cd_eq_arr_vs_scalar() -> bool { baml.ops.equals_equals([1], 1) }

test "driver_primitives_and_gate" {
    assert.is_true(cd_eq_int_same())
    assert.is_true(!cd_eq_int_diff())
    assert.is_true(cd_eq_string_same())
    assert.is_true(!cd_eq_string_diff())
    assert.is_true(cd_eq_bool())
    assert.is_true(!cd_eq_cross_type())
    assert.is_true(!cd_eq_arr_vs_scalar())
}

// Enum identity.
function cd_eq_enum_same() -> bool { baml.ops.equals_equals(CdColor.Red, CdColor.Red) }
function cd_eq_enum_diff() -> bool { baml.ops.equals_equals(CdColor.Red, CdColor.Green) }

test "driver_enum_identity" {
    assert.is_true(cd_eq_enum_same())
    assert.is_true(!cd_eq_enum_diff())
}

// Structural classes (distinct instances, equal fields → equal).
function cd_eq_class_same() -> bool { baml.ops.equals_equals(cd_mk_point(1, 2), cd_mk_point(1, 2)) }
function cd_eq_class_diff() -> bool { baml.ops.equals_equals(cd_mk_point(1, 2), cd_mk_point(1, 3)) }
function cd_eq_class_nested() -> bool { baml.ops.equals_equals(cd_mk_line(0, 0, 1, 1), cd_mk_line(0, 0, 1, 1)) }
function cd_eq_class_nested_diff() -> bool { baml.ops.equals_equals(cd_mk_line(0, 0, 1, 1), cd_mk_line(0, 0, 9, 1)) }

test "driver_structural_classes" {
    assert.is_true(cd_eq_class_same())
    assert.is_true(!cd_eq_class_diff())
    assert.is_true(cd_eq_class_nested())
    assert.is_true(!cd_eq_class_nested_diff())
}

// Containers recurse structurally.
function cd_eq_arr_same() -> bool { baml.ops.equals_equals([1, 2, 3], [1, 2, 3]) }
function cd_eq_arr_diff() -> bool { baml.ops.equals_equals([1, 2, 3], [1, 2, 4]) }
function cd_eq_arr_len() -> bool { baml.ops.equals_equals([1, 2], [1, 2, 3]) }
function cd_eq_arr_nested() -> bool { baml.ops.equals_equals([[1], [2]], [[1], [2]]) }
function cd_eq_arr_of_class() -> bool { baml.ops.equals_equals([cd_mk_point(1, 2)], [cd_mk_point(1, 2)]) }
function cd_eq_map_same() -> bool {
    let a = { "a": 1, "b": 2 };
    let b = { "b": 2, "a": 1 };
    baml.ops.equals_equals(a, b)
}
function cd_eq_map_diff() -> bool {
    let a = { "a": 1 };
    let b = { "a": 2 };
    baml.ops.equals_equals(a, b)
}

test "driver_containers" {
    assert.is_true(cd_eq_arr_same())
    assert.is_true(!cd_eq_arr_diff())
    assert.is_true(!cd_eq_arr_len())
    assert.is_true(cd_eq_arr_nested())
    assert.is_true(cd_eq_arr_of_class())
    assert.is_true(cd_eq_map_same())
    assert.is_true(!cd_eq_map_diff())
}

// Reflexivity: NaN == NaN, so a value containing NaN equals itself and equals a
// structurally identical value. `cd_eq_float_same` is the ordinary sanity check;
// `cd_eq_signed_zeros` pins that `-0.0` and `0.0` are one value.
function cd_eq_float_same() -> bool { baml.ops.equals_equals([1.5], [1.5]) }
function cd_eq_float_diff() -> bool { baml.ops.equals_equals([1.5], [2.5]) }
function cd_eq_nan_pair() -> bool { baml.ops.equals_equals([float.nan()], [float.nan()]) }
function cd_eq_nan_self() -> bool {
    let arr = [float.nan()];
    baml.ops.equals_equals(arr, arr)
}
function cd_eq_nan_vs_number() -> bool { baml.ops.equals_equals([float.nan()], [1.5]) }
function cd_eq_signed_zeros() -> bool { baml.ops.equals_equals([-0.0], [0.0]) }

test "driver_reflexive_nan" {
    assert.is_true(cd_eq_float_same())
    assert.is_true(!cd_eq_float_diff())
    // Two distinct arrays each holding NaN: NaN == NaN ⇒ equal. This is the
    // leaf comparison, not the same-pointer shortcut — the arrays differ.
    assert.is_true(cd_eq_nan_pair())
    // The SAME array holding NaN, compared to itself: equal by reflexivity.
    assert.is_true(cd_eq_nan_self())
    // NaN is still distinct from every number.
    assert.is_true(!cd_eq_nan_vs_number())
    // `-0.0` and `0.0` are one value, as under IEEE.
    assert.is_true(cd_eq_signed_zeros())
}

// ─── `bool`'s Compare, reached by method call ────────────────────────────────
// Regression: `bool`'s `Compare` must be reflexive at the equal-operand
// boundary. The stdlib once overrode `ge(self, _) = self` / `le(self, _) = !self`,
// so `false >= false` and `true <= true` wrongly returned `false`. Dropping
// those overrides lets `bool` inherit the `Compare` defaults, which read `cmp`
// and are reflexive because `cmp(x, x)` is `Ordering.Equal`. Dispatched through
// a `bool`-typed method call, so it exercises the inherited default bodies.

function cd_m_ge_b(a: bool, b: bool) -> bool { a.ge(b) }
function cd_m_le_b(a: bool, b: bool) -> bool { a.le(b) }
function cd_m_ge_false_false() -> bool { cd_m_ge_b(false, false) }
function cd_m_le_true_true() -> bool { cd_m_le_b(true, true) }
function cd_m_ge_true_false() -> bool { cd_m_ge_b(true, false) }
function cd_m_ge_false_true() -> bool { cd_m_ge_b(false, true) }
function cd_m_le_false_true() -> bool { cd_m_le_b(false, true) }
function cd_m_le_true_false() -> bool { cd_m_le_b(true, false) }

test "bool_compare_is_reflexive" {
    // Reflexive (the bug): `x >= x` and `x <= x` are always true.
    assert.is_true(cd_m_ge_false_false())
    assert.is_true(cd_m_le_true_true())
    // The strict directions stay correct.
    assert.is_true(cd_m_ge_true_false())
    assert.is_true(!cd_m_ge_false_true())
    assert.is_true(cd_m_le_false_true())
    assert.is_true(!cd_m_le_true_false())
}

// ─── Custom `Equals` dispatch ────────────────────────────────────────────────
// The driver dispatches a class's user-defined `eq`, not structural comparison.
// `CdTag.eq` compares only `id` (ignoring `note`), so equal-id/different-note is
// `true` (structural would be `false`) — proving the dispatch goes through `eq`.

class CdTag {
    id: int
    note: string
}

implement baml.ops.Equals for CdTag {
    function eq(self, other: Self) -> bool throws never { self.id == other.id }
}

function cd_mk_tag(id: int, note: string) -> CdTag { CdTag { id: id, note: note } }
function cd_eq_same_id_diff_note() -> bool { baml.ops.equals_equals(cd_mk_tag(1, "a"), cd_mk_tag(1, "b")) }
function cd_eq_diff_id() -> bool { baml.ops.equals_equals(cd_mk_tag(1, "a"), cd_mk_tag(2, "a")) }
function cd_eq_same() -> bool { baml.ops.equals_equals(cd_mk_tag(7, "x"), cd_mk_tag(7, "x")) }

test "driver_dispatches_custom_class_equals" {
    // Custom `eq` ignores `note`, so equal `id` ⇒ equal even with different notes.
    assert.is_true(cd_eq_same_id_diff_note())
    assert.is_true(cd_eq_same())
    // Different `id` ⇒ the custom `eq` returns false.
    assert.is_true(!cd_eq_diff_id())
}

// A class WITHOUT a custom `Equals` still compares structurally (the fallback
// when the resolver finds no impl) — and a custom-`Equals` class nested as a
// field of a plain class is reached through structural recursion (a nested
// yield).

class CdNestedTag {
    id: int
    note: string
}

implement baml.ops.Equals for CdNestedTag {
    function eq(self, other: Self) -> bool throws never { self.id == other.id }
}

class CdWrapper {
    tag: CdNestedTag
    label: string
}

function cd_mk_wrapper(id: int, note: string, label: string) -> CdWrapper {
    CdWrapper { tag: CdNestedTag { id: id, note: note }, label: label }
}

// CdWrapper has no custom Equals → structural: compares `tag` (via
// CdNestedTag.eq, which ignores note) and `label`.
function cd_eq_nested_tag_custom() -> bool { baml.ops.equals_equals(cd_mk_wrapper(1, "a", "L"), cd_mk_wrapper(1, "b", "L")) }
function cd_eq_nested_label_differs() -> bool { baml.ops.equals_equals(cd_mk_wrapper(1, "a", "L"), cd_mk_wrapper(1, "a", "M")) }

test "driver_custom_equals_nested_in_structural_class" {
    // tag equal via custom eq (note ignored), label equal ⇒ CdWrapper equal.
    assert.is_true(cd_eq_nested_tag_custom())
    // label differs ⇒ CdWrapper structurally unequal even though tags are eq.
    assert.is_true(!cd_eq_nested_label_differs())
}

// Generic class with a blanket `Equals`: the driver resolves the impl at the
// instance's `class_type_args` (so `CdBox<int>`'s `eq` runs with `T = int`), and
// the custom `eq` body itself recurses into `baml.ops.equals_equals` (a nested
// yield through the trampoline).

class CdBox<T> {
    v: T
}

implement<T> baml.ops.Equals for CdBox<T> {
    function eq(self, other: Self) -> bool throws never {
        baml.ops.equals_equals(self.v, other.v)
    }
}

function cd_mk_box_int(v: int) -> CdBox<int> { CdBox<int> { v: v } }
function cd_eq_box_int_same() -> bool { baml.ops.equals_equals(cd_mk_box_int(5), cd_mk_box_int(5)) }
function cd_eq_box_int_diff() -> bool { baml.ops.equals_equals(cd_mk_box_int(5), cd_mk_box_int(6)) }

test "driver_dispatches_generic_class_equals" {
    assert.is_true(cd_eq_box_int_same())
    assert.is_true(!cd_eq_box_int_diff())
}

// Different concrete generic instantiations are never equal: `CdBox<int>` and
// `CdBox<string>` are distinct `Self`, so the driver rejects the pair before
// dispatching `eq` (the `class_type_args` gate), never running `eq` with
// mismatched operand types.
function cd_box_int_one() -> CdBox<int> { CdBox<int> { v: 1 } }
function cd_box_str_one() -> CdBox<string> { CdBox<string> { v: "1" } }
function cd_eq_int_vs_str() -> bool { baml.ops.equals_equals(cd_box_int_one(), cd_box_str_one()) }

test "driver_distinct_generic_instantiations_are_unequal" {
    assert.is_true(!cd_eq_int_vs_str())
}

// Custom `Equals` on an enum: `baml.ops.equals_equals` dispatches the enum's
// `eq` rather than comparing variant identity. `CdTotalEq.eq` is total-true, so
// two *different* variants compare equal — which identity never would — proving
// the dispatch reaches the enum's `eq`.

enum CdTotalEq {
    A
    B
    C
}

implement baml.ops.Equals for CdTotalEq {
    function eq(self, other: Self) -> bool throws never { true }
}

function cd_eq_diff_variants() -> bool { baml.ops.equals_equals(CdTotalEq.A, CdTotalEq.B) }

test "driver_dispatches_custom_enum_equals" {
    assert.is_true(cd_eq_diff_variants())
}

// ─── Ordering operators dispatch `baml.ops.Compare` ─────────────────────────
//
// `<` `<=` `>` `>=` on operands the comparison opcodes cannot order lower to a
// `VirtualCall` on `baml.ops.Compare`. Before that route existed they reached
// `exec_cmpop` and aborted with the uncatchable `VmInternalError::CannotApplyCmpOp`
// ("cannot apply comparison operation: bool < bool") — for `bool` that needed no
// user code at all, since the stdlib declares `implement Compare for bool`.

// The operator form must agree with the method form pinned by
// `bool_compare_is_reflexive` above, value for value — same impl, same defaults.
function cd_op_lt_b(a: bool, b: bool) -> bool { a < b }
function cd_op_le_b(a: bool, b: bool) -> bool { a <= b }
function cd_op_gt_b(a: bool, b: bool) -> bool { a > b }
function cd_op_ge_b(a: bool, b: bool) -> bool { a >= b }

function cd_op_lt_false_true() -> bool { cd_op_lt_b(false, true) }
function cd_op_lt_true_false() -> bool { cd_op_lt_b(true, false) }
function cd_op_lt_same() -> bool { cd_op_lt_b(true, true) }
function cd_op_le_same() -> bool { cd_op_le_b(false, false) }
function cd_op_le_false_true() -> bool { cd_op_le_b(false, true) }
function cd_op_le_true_false() -> bool { cd_op_le_b(true, false) }
function cd_op_gt_true_false() -> bool { cd_op_gt_b(true, false) }
function cd_op_gt_same() -> bool { cd_op_gt_b(true, true) }
function cd_op_ge_same() -> bool { cd_op_ge_b(true, true) }
function cd_op_ge_false_true() -> bool { cd_op_ge_b(false, true) }

test "bool_ordering_operators_dispatch_compare" {
    // `false < true` is the only strict-less pair.
    assert.is_true(cd_op_lt_false_true())
    assert.is_true(!cd_op_lt_true_false())
    assert.is_true(!cd_op_lt_same())
    // `le`/`ge` come from the `Compare` defaults and are reflexive.
    assert.is_true(cd_op_le_same())
    assert.is_true(cd_op_ge_same())
    assert.is_true(cd_op_le_false_true())
    assert.is_true(!cd_op_le_true_false())
    // `gt` is `bool`'s own override.
    assert.is_true(cd_op_gt_true_false())
    assert.is_true(!cd_op_gt_same())
    assert.is_true(!cd_op_ge_false_true())
}

// A class implementing only the required `cmp`: `lt`/`le`/`gt`/`ge` must resolve
// to the interface defaults, which are merged into the impl's method table at
// bake time.

class CdMoney {
    cents: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.cents == other.cents }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            if (self.cents < other.cents) {
                baml.ops.Ordering.Less
            } else if (self.cents > other.cents) {
                baml.ops.Ordering.Greater
            } else {
                baml.ops.Ordering.Equal
            }
        }
    }
}

function cd_money(c: int) -> CdMoney { CdMoney { cents: c } }

function cd_cheaper() -> bool { cd_money(1) < cd_money(2) }
function cd_not_cheaper() -> bool { cd_money(2) < cd_money(1) }
function cd_le_equal() -> bool { cd_money(1) <= cd_money(1) }
function cd_gt_bigger() -> bool { cd_money(2) > cd_money(1) }
function cd_ge_equal() -> bool { cd_money(1) >= cd_money(1) }
function cd_ge_smaller() -> bool { cd_money(1) >= cd_money(2) }

test "user_class_ordering_dispatches_compare" {
    assert.is_true(cd_cheaper())
    assert.is_true(!cd_not_cheaper())
    assert.is_true(cd_le_equal())
    assert.is_true(cd_gt_bigger())
    assert.is_true(cd_ge_equal())
    assert.is_true(!cd_ge_smaller())
}

// Each operator dispatches its own method, so an override wins over the default
// it replaces. `gt` here is deliberately inconsistent with `cmp` — the default
// would make `a > a` false, the override makes it true. Pins that `>` is not
// lowered as `!(a <= b)`, as a swapped `b.lt(a)`, nor as a `cmp` call.

class CdOdd {
    n: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.n == other.n }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            if (self.n < other.n) {
                baml.ops.Ordering.Less
            } else if (self.n > other.n) {
                baml.ops.Ordering.Greater
            } else {
                baml.ops.Ordering.Equal
            }
        }
        function gt(self, other: Self) -> bool throws never { true }
    }
}

function cd_odd(n: int) -> CdOdd { CdOdd { n: n } }
function cd_gt_self() -> bool { cd_odd(1) > cd_odd(1) }
function cd_le_self() -> bool { cd_odd(1) <= cd_odd(1) }

test "user_class_ordering_honors_gt_override" {
    assert.is_true(cd_gt_self())
    // The un-overridden `le` still follows the default (`cmp != Greater`).
    assert.is_true(cd_le_self())
}

// Enums can implement `Compare` too; before the route existed `exec_cmpop`'s
// `(Variant, Variant)` arm handled only `Eq`/`Ne` and aborted on ordering.

enum CdOrderedEnum {
    A
    B
}

implement baml.ops.Equals for CdOrderedEnum {
    function eq(self, other: Self) -> bool throws never { false }
}

implement baml.ops.Compare for CdOrderedEnum {
    function cmp(self, other: Self) -> baml.ops.Ordering throws never {
        baml.ops.Ordering.Less
    }
}

function cd_lt_variants() -> bool { CdOrderedEnum.A < CdOrderedEnum.B }
function cd_gt_variants() -> bool { CdOrderedEnum.A > CdOrderedEnum.B }

test "enum_ordering_dispatches_compare" {
    assert.is_true(cd_lt_variants())
    // `gt` reads `cmp`, which this impl pins to `Less` for every pair.
    assert.is_true(!cd_gt_variants())
}

// Inside a `T extends Compare` body the operand type is a type variable, so the
// impl can only come from the runtime instantiation. This shape silently worked
// for primitives before (the VM's opcode arms) and fatally aborted for classes —
// the body looks tested while half its instantiations abort.
// Monomorphic wrappers are required because literal arguments would otherwise
// infer a literal union for `T`, which no `Compare` bound admits.

function cd_lt_g<T extends baml.ops.Compare>(a: T, b: T) -> bool { a < b }
function cd_lt_int(a: int, b: int) -> bool { cd_lt_g(a, b) }
function cd_lt_bool(a: bool, b: bool) -> bool { cd_lt_g(a, b) }
function cd_lt_money(a: CdMoney, b: CdMoney) -> bool { cd_lt_g(a, b) }

function cd_generic_int() -> bool { cd_lt_int(1, 2) }
function cd_generic_bool() -> bool { cd_lt_bool(false, true) }
function cd_generic_money() -> bool { cd_lt_money(cd_money(1), cd_money(2)) }
function cd_generic_money_rev() -> bool { cd_lt_money(cd_money(2), cd_money(1)) }

test "bounded_typevar_ordering_dispatches_at_runtime" {
    assert.is_true(cd_generic_int())
    assert.is_true(cd_generic_bool())
    assert.is_true(cd_generic_money())
    assert.is_true(!cd_generic_money_rev())
}

// Union type args are order-insensitive: `CdUBox<int | string>` and
// `CdUBox<string | int>` are the same `Self`, so two such instances with equal
// contents compare equal — the driver compares `class_type_args` semantically
// (`ty_args_equivalent`), not structurally, the same notion of "same
// instantiation" the resolver and reflection use. `CdUBox` deliberately has no
// `Equals` impl, so the comparison is structural once the gate admits the pair.

class CdUBox<T> {
    v: T
}

function cd_mk_ubox_a() -> CdUBox<int | string> { CdUBox<int | string> { v: 1 } }
function cd_mk_ubox_b() -> CdUBox<string | int> { CdUBox<string | int> { v: 1 } }
function cd_eq_reordered_union() -> bool { baml.ops.equals_equals(cd_mk_ubox_a(), cd_mk_ubox_b()) }

test "driver_union_type_args_order_insensitive" {
    assert.is_true(cd_eq_reordered_union())
}
