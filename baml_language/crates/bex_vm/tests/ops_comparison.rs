//! Unit tests for the scalar `baml.ops.Equals` builtins (`eq` for `int` /
//! `float` / …), invoked directly via `get_native_fn`.
//!
//! This pins the native implementations and their generated glue, independent
//! of the language-level dispatch surface. Two neighbours are covered elsewhere
//! because they are not native: container (`T[]` / `map`) equality is BAML that
//! delegates to `baml.ops.equals_equals`, and `Compare.cmp` is BAML written on
//! the comparison operators. Both are covered end-to-end by
//! `comparison_driver.rs` and the `ns_floats` / `ns_operators` fixtures in
//! `baml_tests`.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{
    BexVm,
    package_baml::{BamlPackageBaml, NativeCallResult, PackageBamlImpl},
};
use bex_vm_types::Value;

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

fn float(vm: &mut BexVm, value: f64) -> Value {
    vm.float_value(value)
}

/// Invoke a `baml.ops.*` comparison builtin and return its boolean result.
fn call_bool(vm: &mut BexVm, path: &str, args: &[Value]) -> bool {
    let f = PackageBamlImpl::get_native_fn(path)
        .unwrap_or_else(|| panic!("native fn {path:?} missing"));
    match f(vm, args) {
        NativeCallResult::Done(v) => v
            .as_bool()
            .unwrap_or_else(|| panic!("{path} did not return a bool: {v:?}")),
        NativeCallResult::Error(e) => panic!("{path} errored: {e:?}"),
        NativeCallResult::YieldToCall { .. } => panic!("{path} unexpectedly yielded"),
    }
}

const INT_EQ: &str = "baml.ops.Equals$for$int.eq";
const FLOAT_EQ: &str = "baml.ops.Equals$for$float.eq";
const STRING_EQ: &str = "baml.ops.Equals$for$string.eq";

#[test]
fn int_eq() {
    let mut vm = make_vm();
    assert!(call_bool(&mut vm, INT_EQ, &[Value::int(5), Value::int(5)]));
    assert!(!call_bool(&mut vm, INT_EQ, &[Value::int(5), Value::int(6)]));
}

#[test]
fn string_eq_compares_by_value() {
    let mut vm = make_vm();
    let a = Value::object(vm.tlab.alloc_string("hi".to_string()));
    let b = Value::object(vm.tlab.alloc_string("hi".to_string()));
    let c = Value::object(vm.tlab.alloc_string("ho".to_string()));
    assert!(call_bool(&mut vm, STRING_EQ, &[a, b]));
    assert!(!call_bool(&mut vm, STRING_EQ, &[a, c]));
}

/// `Equals` is reflexive, so `float`'s `eq` departs from IEEE on NaN: every NaN
/// equals every other NaN (and itself). `-0.0 == 0.0` still holds, as under
/// IEEE. NaNs are boxed (distinct heap boxes, so no same-pointer shortcut can
/// mask a non-reflexive leaf); the numbers here are inline.
#[test]
fn float_eq_is_reflexive() {
    let mut vm = make_vm();
    let one = float(&mut vm, 1.5);
    let one_b = float(&mut vm, 1.5);
    let two = float(&mut vm, 2.5);
    let nan = float(&mut vm, f64::NAN);
    let nan_b = float(&mut vm, f64::from_bits(0xFFF8_0000_DEAD_BEEF));
    let pos_zero = float(&mut vm, 0.0);
    let neg_zero = float(&mut vm, -0.0);
    assert!(one.is_inline_float() && pos_zero.is_inline_float() && neg_zero.is_inline_float());
    assert!(nan.is_object() && nan_b.is_object() && nan != nan_b);

    assert!(call_bool(&mut vm, FLOAT_EQ, &[one, one_b]));
    assert!(!call_bool(&mut vm, FLOAT_EQ, &[one, two]));

    // Reflexive, and every NaN is one value regardless of sign or payload.
    assert!(call_bool(&mut vm, FLOAT_EQ, &[nan, nan]));
    assert!(call_bool(&mut vm, FLOAT_EQ, &[nan, nan_b]));
    // …but still distinct from every number.
    assert!(!call_bool(&mut vm, FLOAT_EQ, &[nan, one]));

    assert!(call_bool(&mut vm, FLOAT_EQ, &[neg_zero, pos_zero]));
}
