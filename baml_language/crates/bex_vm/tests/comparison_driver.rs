//! End-to-end tests for the broad `==` driver `baml.ops.equals` (the may-yield
//! `$rust_function` worklist in `package_baml::ops`).
//!
//! These call `baml.ops.equals(a, b)` directly from BAML and run the program,
//! so they exercise the driver through the real VM (entry point + trampoline),
//! independent of the operator-lowering work. They cover the cases the driver
//! handles today: the concrete-type gate, primitives, enum identity, structural
//! classes, nested containers, and **non-reflexivity** (a value containing NaN
//! is not equal to itself). Dispatch to a user class's custom `Equals` is a
//! separate increment.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

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
    loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => return v.as_bool().expect("equals returns bool"),
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
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

function eq_int_same() -> bool { baml.ops.equals(1, 1) }
function eq_int_diff() -> bool { baml.ops.equals(1, 2) }
function eq_string_same() -> bool { baml.ops.equals("hi", "hi") }
function eq_string_diff() -> bool { baml.ops.equals("hi", "ho") }
function eq_bool() -> bool { baml.ops.equals(true, true) }

// Concrete-type gate: different runtime types are never equal (never an error).
function eq_cross_type() -> bool { baml.ops.equals(1, "1") }
function eq_arr_vs_scalar() -> bool { baml.ops.equals([1], 1) }

// Enum identity.
function eq_enum_same() -> bool { baml.ops.equals(Color.Red, Color.Red) }
function eq_enum_diff() -> bool { baml.ops.equals(Color.Red, Color.Green) }

// Structural classes (distinct instances, equal fields → equal).
function eq_class_same() -> bool { baml.ops.equals(mk_point(1, 2), mk_point(1, 2)) }
function eq_class_diff() -> bool { baml.ops.equals(mk_point(1, 2), mk_point(1, 3)) }
function eq_class_nested() -> bool { baml.ops.equals(mk_line(0, 0, 1, 1), mk_line(0, 0, 1, 1)) }
function eq_class_nested_diff() -> bool { baml.ops.equals(mk_line(0, 0, 1, 1), mk_line(0, 0, 9, 1)) }

// Containers recurse structurally.
function eq_arr_same() -> bool { baml.ops.equals([1, 2, 3], [1, 2, 3]) }
function eq_arr_diff() -> bool { baml.ops.equals([1, 2, 3], [1, 2, 4]) }
function eq_arr_len() -> bool { baml.ops.equals([1, 2], [1, 2, 3]) }
function eq_arr_nested() -> bool { baml.ops.equals([[1], [2]], [[1], [2]]) }
function eq_arr_of_class() -> bool { baml.ops.equals([mk_point(1, 2)], [mk_point(1, 2)]) }
function eq_map_same() -> bool {
    let a = { "a": 1, "b": 2 };
    let b = { "b": 2, "a": 1 };
    baml.ops.equals(a, b)
}
function eq_map_diff() -> bool {
    let a = { "a": 1 };
    let b = { "a": 2 };
    baml.ops.equals(a, b)
}

// Non-reflexivity: NaN != NaN, so a value containing NaN is not equal even to
// itself (the SAME object). `eq_float_same` is the reflexive sanity check.
function eq_float_same() -> bool { baml.ops.equals([1.5], [1.5]) }
function eq_nan_pair() -> bool { baml.ops.equals([float.nan()], [float.nan()]) }
function eq_nan_self() -> bool {
    let arr = [float.nan()];
    baml.ops.equals(arr, arr)
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
