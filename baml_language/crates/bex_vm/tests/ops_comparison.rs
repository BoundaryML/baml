//! Unit tests for the scalar `baml.ops` comparison builtins (`Equals` /
//! `Compare` for `int` / `float` / …), invoked directly via `get_native_fn`.
//!
//! This pins the native implementations and their generated glue, independent
//! of the language-level dispatch surface. Container (`T[]` / `map`) equality is
//! now BAML that delegates to `baml.ops.equals_equals`, so it is covered end-to-end
//! by `comparison_driver.rs` rather than here.

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{
    BexVm,
    package_baml::{BamlPackageBaml, NativeCallResult, PackageBamlImpl},
};
use bex_vm_types::{Value, types::Object};

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

fn alloc_float(vm: &mut BexVm, value: f64) -> Value {
    Value::object(vm.tlab.alloc(Object::Float(value)))
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
const INT_LT: &str = "baml.ops.Compare$for$int.lt";
const FLOAT_EQ: &str = "baml.ops.Equals$for$float.eq";
const FLOAT_LT: &str = "baml.ops.Compare$for$float.lt";
const FLOAT_GT: &str = "baml.ops.Compare$for$float.gt";
const FLOAT_GE: &str = "baml.ops.Compare$for$float.ge";
const FLOAT_LE: &str = "baml.ops.Compare$for$float.le";

#[test]
fn int_eq_and_lt() {
    let mut vm = make_vm();
    assert!(call_bool(&mut vm, INT_EQ, &[Value::int(5), Value::int(5)]));
    assert!(!call_bool(&mut vm, INT_EQ, &[Value::int(5), Value::int(6)]));

    assert!(call_bool(&mut vm, INT_LT, &[Value::int(5), Value::int(6)]));
    assert!(!call_bool(&mut vm, INT_LT, &[Value::int(6), Value::int(5)]));
    assert!(!call_bool(&mut vm, INT_LT, &[Value::int(5), Value::int(5)]));
}

#[test]
fn float_eq_uses_ieee_equality() {
    let mut vm = make_vm();
    let one = alloc_float(&mut vm, 1.5);
    let one_b = alloc_float(&mut vm, 1.5);
    let two = alloc_float(&mut vm, 2.5);
    let nan = alloc_float(&mut vm, f64::NAN);

    assert!(call_bool(&mut vm, FLOAT_EQ, &[one, one_b]));
    assert!(!call_bool(&mut vm, FLOAT_EQ, &[one, two]));
    // IEEE: NaN is not equal to itself.
    assert!(!call_bool(&mut vm, FLOAT_EQ, &[nan, nan]));
}

#[test]
fn float_ordering_is_ieee_for_nan() {
    let mut vm = make_vm();
    let one = alloc_float(&mut vm, 1.0);
    let two = alloc_float(&mut vm, 2.0);
    let nan = alloc_float(&mut vm, f64::NAN);

    assert!(call_bool(&mut vm, FLOAT_LT, &[one, two]));
    assert!(call_bool(&mut vm, FLOAT_GT, &[two, one]));
    assert!(call_bool(&mut vm, FLOAT_GE, &[two, two]));
    assert!(call_bool(&mut vm, FLOAT_LE, &[two, two]));

    // Every ordering comparison against NaN is false — this is exactly why
    // float overrides the interface's boolean-derived `gt`/`ge`/`le` defaults.
    assert!(!call_bool(&mut vm, FLOAT_LT, &[nan, one]));
    assert!(!call_bool(&mut vm, FLOAT_GT, &[nan, one]));
    assert!(!call_bool(&mut vm, FLOAT_GE, &[nan, one]));
    assert!(!call_bool(&mut vm, FLOAT_LE, &[nan, one]));
}
