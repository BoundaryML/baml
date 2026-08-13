//! End-to-end tests for the broad `==` driver `baml.ops.equals_equals` (the may-yield
//! `$rust_function` worklist in `package_baml::ops`).
//!
//! These call `baml.ops.equals_equals(a, b)` directly from BAML and run the program,
//! so they exercise the driver through the real VM (entry point + trampoline),
//! independent of the operator-lowering work. They cover the concrete-type gate,
//! primitives, enum identity, structural classes, nested containers,
//! **non-reflexivity** (a value containing NaN is not equal to itself), and
//! dispatch to a user class's custom `Equals.eq` (resolved against the baked impl
//! registry and called via `YieldToCall`), including generic classes and the
//! structural fallback when a class has no `Equals` impl.
//!
//! It also covers the sibling comparison surface: `baml.ops.Compare`, reached
//! both by method call (`bool_compare_is_reflexive`) and — since ordering
//! operators gained an interface lowering — by `<` `<=` `>` `>=` themselves.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

/// Cap exec-loop iterations so regressions fail fast instead of hanging CI.
const MAX_EXEC_CALLS: usize = 256;

/// Compile `src`, run the no-arg `user.<fn_name>`, and return its `bool` result.
fn run_bool(src: &str, fn_name: &str) -> bool {
    let program = compile_source(src);
    let idx = program
        .function_index(fn_name)
        .unwrap_or_else(|| panic!("function {fn_name:?} not found"));
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let fptr = vm.heap.compile_time_ptr(idx);
    vm.set_entry_point(fptr, &[]);
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => return v.as_bool().expect("equals returns bool"),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

// Class instances are built via helper functions (a class literal can't be a
// direct call argument), and the `eq_*` entry points each return one bool.
const PRELUDE: &str = r#"
enum Color { Red  Green }
class Point { x: int  y: int }
class Line { start: Point  end: Point }

function mk_point(x: int, y: int) -> Point { Point { x: x, y: y } }
function mk_line(ax: int, ay: int, bx: int, by: int) -> Line {
    Line { start: mk_point(ax, ay), end: mk_point(bx, by) }
}

function eq_int_same() -> bool { baml.ops.equals_equals(1, 1) }
function eq_int_diff() -> bool { baml.ops.equals_equals(1, 2) }
function eq_string_same() -> bool { baml.ops.equals_equals("hi", "hi") }
function eq_string_diff() -> bool { baml.ops.equals_equals("hi", "ho") }
function eq_bool() -> bool { baml.ops.equals_equals(true, true) }

// Concrete-type gate: different runtime types are never equal (never an error).
function eq_cross_type() -> bool { baml.ops.equals_equals(1, "1") }
function eq_arr_vs_scalar() -> bool { baml.ops.equals_equals([1], 1) }

// Enum identity.
function eq_enum_same() -> bool { baml.ops.equals_equals(Color.Red, Color.Red) }
function eq_enum_diff() -> bool { baml.ops.equals_equals(Color.Red, Color.Green) }

// Structural classes (distinct instances, equal fields → equal).
function eq_class_same() -> bool { baml.ops.equals_equals(mk_point(1, 2), mk_point(1, 2)) }
function eq_class_diff() -> bool { baml.ops.equals_equals(mk_point(1, 2), mk_point(1, 3)) }
function eq_class_nested() -> bool { baml.ops.equals_equals(mk_line(0, 0, 1, 1), mk_line(0, 0, 1, 1)) }
function eq_class_nested_diff() -> bool { baml.ops.equals_equals(mk_line(0, 0, 1, 1), mk_line(0, 0, 9, 1)) }

// Containers recurse structurally.
function eq_arr_same() -> bool { baml.ops.equals_equals([1, 2, 3], [1, 2, 3]) }
function eq_arr_diff() -> bool { baml.ops.equals_equals([1, 2, 3], [1, 2, 4]) }
function eq_arr_len() -> bool { baml.ops.equals_equals([1, 2], [1, 2, 3]) }
function eq_arr_nested() -> bool { baml.ops.equals_equals([[1], [2]], [[1], [2]]) }
function eq_arr_of_class() -> bool { baml.ops.equals_equals([mk_point(1, 2)], [mk_point(1, 2)]) }
function eq_map_same() -> bool {
    let a = { "a": 1, "b": 2 };
    let b = { "b": 2, "a": 1 };
    baml.ops.equals_equals(a, b)
}
function eq_map_diff() -> bool {
    let a = { "a": 1 };
    let b = { "a": 2 };
    baml.ops.equals_equals(a, b)
}

// Non-reflexivity: NaN != NaN, so a value containing NaN is not equal even to
// itself (the SAME object). `eq_float_same` is the reflexive sanity check.
function eq_float_same() -> bool { baml.ops.equals_equals([1.5], [1.5]) }
function eq_nan_pair() -> bool { baml.ops.equals_equals([float.nan()], [float.nan()]) }
function eq_nan_self() -> bool {
    let arr = [float.nan()];
    baml.ops.equals_equals(arr, arr)
}
"#;

#[test]
fn driver_primitives_and_gate() {
    assert!(run_bool(PRELUDE, "user.eq_int_same"));
    assert!(!run_bool(PRELUDE, "user.eq_int_diff"));
    assert!(run_bool(PRELUDE, "user.eq_string_same"));
    assert!(!run_bool(PRELUDE, "user.eq_string_diff"));
    assert!(run_bool(PRELUDE, "user.eq_bool"));
    assert!(!run_bool(PRELUDE, "user.eq_cross_type"));
    assert!(!run_bool(PRELUDE, "user.eq_arr_vs_scalar"));
}

#[test]
fn driver_enum_identity() {
    assert!(run_bool(PRELUDE, "user.eq_enum_same"));
    assert!(!run_bool(PRELUDE, "user.eq_enum_diff"));
}

#[test]
fn driver_structural_classes() {
    assert!(run_bool(PRELUDE, "user.eq_class_same"));
    assert!(!run_bool(PRELUDE, "user.eq_class_diff"));
    assert!(run_bool(PRELUDE, "user.eq_class_nested"));
    assert!(!run_bool(PRELUDE, "user.eq_class_nested_diff"));
}

#[test]
fn driver_containers() {
    assert!(run_bool(PRELUDE, "user.eq_arr_same"));
    assert!(!run_bool(PRELUDE, "user.eq_arr_diff"));
    assert!(!run_bool(PRELUDE, "user.eq_arr_len"));
    assert!(run_bool(PRELUDE, "user.eq_arr_nested"));
    assert!(run_bool(PRELUDE, "user.eq_arr_of_class"));
    assert!(run_bool(PRELUDE, "user.eq_map_same"));
    assert!(!run_bool(PRELUDE, "user.eq_map_diff"));
}

#[test]
fn driver_non_reflexive_nan() {
    assert!(run_bool(PRELUDE, "user.eq_float_same"));
    // Two distinct arrays each holding NaN: NaN != NaN ⇒ unequal.
    assert!(!run_bool(PRELUDE, "user.eq_nan_pair"));
    // The SAME array holding NaN, compared to itself: still unequal (no
    // same-pointer shortcut, because equality is not reflexive).
    assert!(!run_bool(PRELUDE, "user.eq_nan_self"));
}

// Regression: `bool`'s `Compare` must be reflexive at the equal-operand boundary.
// The stdlib once overrode `ge(self, _) = self` / `le(self, _) = !self`, so
// `false >= false` and `true <= true` wrongly returned `false`. Dropping those
// overrides lets `bool` inherit the `Compare` defaults (`ge = !lt`, `le = lt || eq`),
// which are reflexive. Dispatched through a `bool`-typed method call, so it exercises
// the inherited default bodies via the real VM.
#[test]
fn bool_compare_is_reflexive() {
    const SRC: &str = r#"
        function ge_b(a: bool, b: bool) -> bool { a.ge(b) }
        function le_b(a: bool, b: bool) -> bool { a.le(b) }
        function ge_false_false() -> bool { ge_b(false, false) }
        function le_true_true() -> bool { le_b(true, true) }
        function ge_true_false() -> bool { ge_b(true, false) }
        function ge_false_true() -> bool { ge_b(false, true) }
        function le_false_true() -> bool { le_b(false, true) }
        function le_true_false() -> bool { le_b(true, false) }
    "#;
    // Reflexive (the bug): `x >= x` and `x <= x` are always true.
    assert!(run_bool(SRC, "user.ge_false_false"));
    assert!(run_bool(SRC, "user.le_true_true"));
    // The strict directions stay correct.
    assert!(run_bool(SRC, "user.ge_true_false")); // true >= false
    assert!(!run_bool(SRC, "user.ge_false_true")); // false < true ⇒ !(false >= true)
    assert!(run_bool(SRC, "user.le_false_true")); // false <= true
    assert!(!run_bool(SRC, "user.le_true_false")); // true > false ⇒ !(true <= false)
}

// Custom `Equals`: the driver dispatches a class's user-defined `eq`, not structural
// comparison. `Tag.eq` compares only `id` (ignoring `note`), so equal-id/different-note
// is `true` (structural would be `false`) — proving the dispatch goes through `eq`.
#[test]
fn driver_dispatches_custom_class_equals() {
    const SRC: &str = r#"
        class Tag { id: int  note: string }
        implement baml.ops.Equals for Tag {
            function eq(self, other: Self) -> bool throws never { self.id == other.id }
        }
        function mk(id: int, note: string) -> Tag { Tag { id: id, note: note } }
        function eq_same_id_diff_note() -> bool { baml.ops.equals_equals(mk(1, "a"), mk(1, "b")) }
        function eq_diff_id() -> bool { baml.ops.equals_equals(mk(1, "a"), mk(2, "a")) }
        function eq_same() -> bool { baml.ops.equals_equals(mk(7, "x"), mk(7, "x")) }
    "#;
    // Custom `eq` ignores `note`, so equal `id` ⇒ equal even with different notes.
    assert!(run_bool(SRC, "user.eq_same_id_diff_note"));
    assert!(run_bool(SRC, "user.eq_same"));
    // Different `id` ⇒ the custom `eq` returns false.
    assert!(!run_bool(SRC, "user.eq_diff_id"));
}

// A class WITHOUT a custom `Equals` still compares structurally (the fallback when the
// resolver finds no impl) — and a custom-`Equals` class nested as a field of a plain
// class is reached through structural recursion (a nested yield).
#[test]
fn driver_custom_equals_nested_in_structural_class() {
    const SRC: &str = r#"
        class Tag { id: int  note: string }
        implement baml.ops.Equals for Tag {
            function eq(self, other: Self) -> bool throws never { self.id == other.id }
        }
        class Wrapper { tag: Tag  label: string }
        function mk(id: int, note: string, label: string) -> Wrapper {
            Wrapper { tag: Tag { id: id, note: note }, label: label }
        }
        // Wrapper has no custom Equals → structural: compares `tag` (via Tag.eq, which
        // ignores note) and `label`.
        function eq_nested_tag_custom() -> bool { baml.ops.equals_equals(mk(1, "a", "L"), mk(1, "b", "L")) }
        function eq_nested_label_differs() -> bool { baml.ops.equals_equals(mk(1, "a", "L"), mk(1, "a", "M")) }
    "#;
    // tag equal via custom eq (note ignored), label equal ⇒ Wrapper equal.
    assert!(run_bool(SRC, "user.eq_nested_tag_custom"));
    // label differs ⇒ Wrapper structurally unequal even though tags are eq.
    assert!(!run_bool(SRC, "user.eq_nested_label_differs"));
}

// Generic class with a blanket `Equals`: the driver resolves the impl at the instance's
// `class_type_args` (so `Box<int>`'s `eq` runs with `T = int`), and the custom `eq` body
// itself recurses into `baml.ops.equals_equals` (a nested yield through the trampoline).
#[test]
fn driver_dispatches_generic_class_equals() {
    const SRC: &str = r#"
        class Box<T> { v: T }
        implement<T> baml.ops.Equals for Box<T> {
            function eq(self, other: Self) -> bool throws never {
                baml.ops.equals_equals(self.v, other.v)
            }
        }
        function mk_int(v: int) -> Box<int> { Box<int> { v: v } }
        function eq_box_int_same() -> bool { baml.ops.equals_equals(mk_int(5), mk_int(5)) }
        function eq_box_int_diff() -> bool { baml.ops.equals_equals(mk_int(5), mk_int(6)) }
    "#;
    assert!(run_bool(SRC, "user.eq_box_int_same"));
    assert!(!run_bool(SRC, "user.eq_box_int_diff"));
}

// Different concrete generic instantiations are never equal: `Box<int>` and `Box<string>`
// are distinct `Self`, so the driver rejects the pair before dispatching `eq` (the
// `class_type_args` gate), never running `eq` with mismatched operand types.
#[test]
fn driver_distinct_generic_instantiations_are_unequal() {
    const SRC: &str = r#"
        class Box<T> { v: T }
        implement<T> baml.ops.Equals for Box<T> {
            function eq(self, other: Self) -> bool throws never {
                baml.ops.equals_equals(self.v, other.v)
            }
        }
        function mk_int() -> Box<int> { Box<int> { v: 1 } }
        function mk_str() -> Box<string> { Box<string> { v: "1" } }
        function eq_int_vs_str() -> bool { baml.ops.equals_equals(mk_int(), mk_str()) }
    "#;
    assert!(!run_bool(SRC, "user.eq_int_vs_str"));
}

// Custom `Equals` on an enum: `baml.ops.equals_equals` dispatches the enum's `eq` rather than
// comparing variant identity. `E.eq` is total-true, so two *different* variants compare
// equal — which identity never would — proving the dispatch reaches the enum's `eq`.
#[test]
fn driver_dispatches_custom_enum_equals() {
    const SRC: &str = r#"
        enum E { A B C }
        implement baml.ops.Equals for E {
            function eq(self, other: Self) -> bool throws never { true }
        }
        function eq_diff_variants() -> bool { baml.ops.equals_equals(E.A, E.B) }
    "#;
    assert!(run_bool(SRC, "user.eq_diff_variants"));
}

// ── Ordering operators dispatch `baml.ops.Compare` ──────────────────────────
//
// `<` `<=` `>` `>=` on operands the comparison opcodes cannot order lower to a
// `VirtualCall` on `baml.ops.Compare`. Before that route existed they reached
// `exec_cmpop` and aborted with the uncatchable `VmInternalError::CannotApplyCmpOp`
// ("cannot apply comparison operation: bool < bool") — for `bool` that needed no
// user code at all, since the stdlib declares `implement Compare for bool`.

// The operator form must agree with the method form pinned by
// `bool_compare_is_reflexive` above, value for value — same impl, same defaults.
#[test]
fn bool_ordering_operators_dispatch_compare() {
    const SRC: &str = r#"
        function lt_b(a: bool, b: bool) -> bool { a < b }
        function le_b(a: bool, b: bool) -> bool { a <= b }
        function gt_b(a: bool, b: bool) -> bool { a > b }
        function ge_b(a: bool, b: bool) -> bool { a >= b }

        function lt_false_true() -> bool { lt_b(false, true) }
        function lt_true_false() -> bool { lt_b(true, false) }
        function lt_same() -> bool { lt_b(true, true) }
        function le_same() -> bool { le_b(false, false) }
        function le_false_true() -> bool { le_b(false, true) }
        function le_true_false() -> bool { le_b(true, false) }
        function gt_true_false() -> bool { gt_b(true, false) }
        function gt_same() -> bool { gt_b(true, true) }
        function ge_same() -> bool { ge_b(true, true) }
        function ge_false_true() -> bool { ge_b(false, true) }
    "#;
    // `false < true` is the only strict-less pair.
    assert!(run_bool(SRC, "user.lt_false_true"));
    assert!(!run_bool(SRC, "user.lt_true_false"));
    assert!(!run_bool(SRC, "user.lt_same"));
    // `le`/`ge` come from the `Compare` defaults and are reflexive.
    assert!(run_bool(SRC, "user.le_same"));
    assert!(run_bool(SRC, "user.ge_same"));
    assert!(run_bool(SRC, "user.le_false_true"));
    assert!(!run_bool(SRC, "user.le_true_false"));
    // `gt` is `bool`'s own override.
    assert!(run_bool(SRC, "user.gt_true_false"));
    assert!(!run_bool(SRC, "user.gt_same"));
    assert!(!run_bool(SRC, "user.ge_false_true"));
}

// A class implementing only the required `lt`: `le`/`gt`/`ge` must resolve to the
// interface defaults, which are merged into the impl's method table at bake time.
#[test]
fn user_class_ordering_dispatches_compare() {
    const SRC: &str = r#"
        class Money {
            cents: int
            implements baml.ops.Equals {
                function eq(self, other: Self) -> bool throws never { self.cents == other.cents }
            }
            implements baml.ops.Compare {
                function lt(self, other: Self) -> bool throws never { self.cents < other.cents }
            }
        }
        function money(c: int) -> Money { Money { cents: c } }

        function cheaper() -> bool { money(1) < money(2) }
        function not_cheaper() -> bool { money(2) < money(1) }
        function le_equal() -> bool { money(1) <= money(1) }
        function gt_bigger() -> bool { money(2) > money(1) }
        function ge_equal() -> bool { money(1) >= money(1) }
        function ge_smaller() -> bool { money(1) >= money(2) }
    "#;
    assert!(run_bool(SRC, "user.cheaper"));
    assert!(!run_bool(SRC, "user.not_cheaper"));
    assert!(run_bool(SRC, "user.le_equal"));
    assert!(run_bool(SRC, "user.gt_bigger"));
    assert!(run_bool(SRC, "user.ge_equal"));
    assert!(!run_bool(SRC, "user.ge_smaller"));
}

// Each operator dispatches its own method, so an override wins over the default it
// replaces. `gt` here is deliberately inconsistent with `!le` — the default would
// make `a > a` false, the override makes it true. Pins that `>` is not lowered as
// `!(a <= b)` or as a swapped `b.lt(a)`.
#[test]
fn user_class_ordering_honors_gt_override() {
    const SRC: &str = r#"
        class Odd {
            n: int
            implements baml.ops.Equals {
                function eq(self, other: Self) -> bool throws never { self.n == other.n }
            }
            implements baml.ops.Compare {
                function lt(self, other: Self) -> bool throws never { self.n < other.n }
                function gt(self, other: Self) -> bool throws never { true }
            }
        }
        function odd(n: int) -> Odd { Odd { n: n } }
        function gt_self() -> bool { odd(1) > odd(1) }
        function le_self() -> bool { odd(1) <= odd(1) }
    "#;
    assert!(run_bool(SRC, "user.gt_self"));
    // The un-overridden `le` still follows the default (`lt || eq`).
    assert!(run_bool(SRC, "user.le_self"));
}

// Enums can implement `Compare` too; before the route existed `exec_cmpop`'s
// `(Variant, Variant)` arm handled only `Eq`/`Ne` and aborted on ordering.
#[test]
fn enum_ordering_dispatches_compare() {
    const SRC: &str = r#"
        enum E { A B }
        implement baml.ops.Equals for E {
            function eq(self, other: Self) -> bool throws never { false }
        }
        implement baml.ops.Compare for E {
            function lt(self, other: Self) -> bool throws never { true }
        }
        function lt_variants() -> bool { E.A < E.B }
        function gt_variants() -> bool { E.A > E.B }
    "#;
    assert!(run_bool(SRC, "user.lt_variants"));
    // `gt` defaults to `!le` = `!(lt || eq)` = `!(true || false)`.
    assert!(!run_bool(SRC, "user.gt_variants"));
}

// Inside a `T extends Compare` body the operand type is a type variable, so the impl
// can only come from the runtime instantiation. This shape silently worked for
// primitives before (the VM's opcode arms) and fatally aborted for classes — the
// body looks tested while half its instantiations abort.
// Monomorphic wrappers are required because literal arguments would otherwise infer a
// literal union for `T`, which no `Compare` bound admits.
#[test]
fn bounded_typevar_ordering_dispatches_at_runtime() {
    const SRC: &str = r#"
        class Money {
            cents: int
            implements baml.ops.Equals {
                function eq(self, other: Self) -> bool throws never { self.cents == other.cents }
            }
            implements baml.ops.Compare {
                function lt(self, other: Self) -> bool throws never { self.cents < other.cents }
            }
        }
        function money(c: int) -> Money { Money { cents: c } }

        function lt_g<T extends baml.ops.Compare>(a: T, b: T) -> bool { a < b }
        function lt_int(a: int, b: int) -> bool { lt_g(a, b) }
        function lt_bool(a: bool, b: bool) -> bool { lt_g(a, b) }
        function lt_money(a: Money, b: Money) -> bool { lt_g(a, b) }

        function generic_int() -> bool { lt_int(1, 2) }
        function generic_bool() -> bool { lt_bool(false, true) }
        function generic_money() -> bool { lt_money(money(1), money(2)) }
        function generic_money_rev() -> bool { lt_money(money(2), money(1)) }
    "#;
    assert!(run_bool(SRC, "user.generic_int"));
    assert!(run_bool(SRC, "user.generic_bool"));
    assert!(run_bool(SRC, "user.generic_money"));
    assert!(!run_bool(SRC, "user.generic_money_rev"));
}

// Union type args are order-insensitive: `Box<int | string>` and `Box<string | int>` are
// the same `Self`, so two such instances with equal contents compare equal — the driver
// compares `class_type_args` semantically (`ty_args_equivalent`), not structurally, the
// same notion of "same instantiation" the resolver and reflection use.
#[test]
fn driver_union_type_args_order_insensitive() {
    const SRC: &str = r#"
        class Box<T> { v: T }
        function mk_a() -> Box<int | string> { Box<int | string> { v: 1 } }
        function mk_b() -> Box<string | int> { Box<string | int> { v: 1 } }
        function eq_reordered_union() -> bool { baml.ops.equals_equals(mk_a(), mk_b()) }
    "#;
    assert!(run_bool(SRC, "user.eq_reordered_union"));
}
