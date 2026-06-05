//! End-to-end tests for the `baml.ops` comparison builtins (the `Equals` /
//! `Compare` impls in `ns_ops/comparison.baml`).
//!
//! The native function pointer is resolved directly via `get_native_fn` and
//! invoked, which bypasses the language-level dispatch surface (still in
//! flight) so these tests pin the native implementations and their generated
//! glue rather than the compiler's method resolution.
//!
//! The container cases double as a regression test for the glue: the per-
//! container lock is a non-reentrant exclusive spin-lock, so comparing a
//! container against *itself* (`a.eq(a)`, both args aliasing one heap object)
//! must not deadlock. The glue snapshots each operand to an owned value
//! instead of holding two overlapping read guards.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_str::BexStr;
use bex_vm::{
    BexVm,
    package_baml::{BamlPackageBaml, NativeCallResult, PackageBamlImpl},
};
use bex_vm_types::{Value, types::Object};
use indexmap::IndexMap;

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

fn alloc_float(vm: &mut BexVm, value: f64) -> Value {
    Value::object(vm.tlab.alloc(Object::Float(value)))
}

fn alloc_array(vm: &mut BexVm, values: Vec<Value>) -> Value {
    Value::object(vm.tlab.alloc(Object::Array(values.into())))
}

fn alloc_map(vm: &mut BexVm, entries: Vec<(&str, Value)>) -> Value {
    let map: IndexMap<BexStr, Value> = entries
        .into_iter()
        .map(|(k, v)| (BexStr::from(k), v))
        .collect();
    Value::object(vm.tlab.alloc(Object::Map(map.into())))
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
const ARRAY_EQ: &str = "baml.ops.Equals$for$T[].eq";
const MAP_EQ: &str = "baml.ops.Equals$for$map<K, V>.eq";

// ── scalars ────────────────────────────────────────────────────────────────

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
    // IEEE: NaN is not equal to itself (deliberately unlike `deep_equals`).
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

// ── containers ───────────────────────────────────────────────────────────

#[test]
fn array_eq_structural_and_distinct() {
    let mut vm = make_vm();
    let a = alloc_array(&mut vm, vec![Value::int(1), Value::int(2), Value::int(3)]);
    let equal = alloc_array(&mut vm, vec![Value::int(1), Value::int(2), Value::int(3)]);
    let different = alloc_array(&mut vm, vec![Value::int(1), Value::int(2), Value::int(4)]);
    let shorter = alloc_array(&mut vm, vec![Value::int(1), Value::int(2)]);

    assert!(call_bool(&mut vm, ARRAY_EQ, &[a, equal]));
    assert!(!call_bool(&mut vm, ARRAY_EQ, &[a, different]));
    assert!(!call_bool(&mut vm, ARRAY_EQ, &[a, shorter]));
}

#[test]
fn array_eq_recurses_into_nested_containers() {
    let mut vm = make_vm();
    let inner1 = alloc_array(&mut vm, vec![Value::int(1)]);
    let inner2 = alloc_array(&mut vm, vec![Value::int(2)]);
    let inner1_copy = alloc_array(&mut vm, vec![Value::int(1)]);
    let inner2_copy = alloc_array(&mut vm, vec![Value::int(2)]);

    let nested = alloc_array(&mut vm, vec![inner1, inner2]);
    let nested_equal = alloc_array(&mut vm, vec![inner1_copy, inner2_copy]);

    // Distinct inner arrays with equal contents must compare equal: the
    // element comparison recurses rather than comparing by reference.
    assert!(call_bool(&mut vm, ARRAY_EQ, &[nested, nested_equal]));
}

#[test]
fn array_eq_against_itself_does_not_deadlock() {
    let mut vm = make_vm();
    let a = alloc_array(&mut vm, vec![Value::int(1), Value::int(2), Value::int(3)]);
    // Both operands alias the same heap array. With overlapping read guards
    // this would spin forever on the container lock; the owned-snapshot glue
    // makes it return cleanly.
    assert!(call_bool(&mut vm, ARRAY_EQ, &[a, a]));
}

#[test]
fn map_eq_structural_order_insensitive_and_self() {
    let mut vm = make_vm();
    let a = alloc_map(&mut vm, vec![("x", Value::int(1)), ("y", Value::int(2))]);
    // Same keys/values inserted in a different order — maps are equal.
    let reordered = alloc_map(&mut vm, vec![("y", Value::int(2)), ("x", Value::int(1))]);
    let different = alloc_map(&mut vm, vec![("x", Value::int(1)), ("y", Value::int(9))]);
    let missing_key = alloc_map(&mut vm, vec![("x", Value::int(1))]);

    assert!(call_bool(&mut vm, MAP_EQ, &[a, reordered]));
    assert!(!call_bool(&mut vm, MAP_EQ, &[a, different]));
    assert!(!call_bool(&mut vm, MAP_EQ, &[a, missing_key]));
    // Aliased operands must not deadlock (same lock concern as arrays).
    assert!(call_bool(&mut vm, MAP_EQ, &[a, a]));
}
