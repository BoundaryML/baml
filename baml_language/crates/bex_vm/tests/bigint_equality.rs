//! Tests that the broad `==` driver (`baml.ops.equals_equals`) compares
//! `Object::Bigint` by numeric value rather than `Arc` identity.
//!
//! `==` on two statically-typed bigints lowers to the specialized `CmpBigintOp`
//! opcode, so the driver's bigint arm is only reached through erased operands
//! (`unknown`, unions, interfaces) or as an element of a structural comparison.
//! Driving it from BAML source cannot guarantee the two operands are distinct
//! allocations — equal literals may share a constant-pool entry — so these
//! allocate the `Arc<BigInt>`s directly and invoke the native fn.

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{
    BexVm,
    package_baml::{BamlPackageBaml, NativeCallResult, PackageBamlImpl},
};
use bex_vm_types::{Value, types::Object};
use num_bigint::BigInt;

const EQUALS_EQUALS: &str = "baml.ops.equals_equals";

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

fn alloc_bigint(vm: &mut BexVm, value: BigInt) -> Value {
    let arc = Arc::new(value);
    let ptr = vm.tlab.alloc(Object::Bigint(arc));
    Value::object(ptr)
}

/// Invoke the equality driver on two operands and return its boolean result.
/// Bigints are leaves, so the driver decides without yielding to bytecode.
fn equals(vm: &mut BexVm, a: Value, b: Value) -> bool {
    let f = PackageBamlImpl::get_native_fn(EQUALS_EQUALS)
        .unwrap_or_else(|| panic!("native fn {EQUALS_EQUALS:?} missing"));
    match f(vm, &[a, b]) {
        NativeCallResult::Done(v) => v
            .as_bool()
            .unwrap_or_else(|| panic!("{EQUALS_EQUALS} did not return a bool: {v:?}")),
        NativeCallResult::Error(e) => panic!("{EQUALS_EQUALS} errored: {e:?}"),
        NativeCallResult::YieldToCall { .. } => {
            panic!("{EQUALS_EQUALS} unexpectedly yielded on a bigint pair")
        }
    }
}

#[test]
fn bigint_distinct_arcs_same_value() {
    let mut vm = make_vm();
    let a = alloc_bigint(&mut vm, BigInt::from(42));
    let b = alloc_bigint(&mut vm, BigInt::from(42));
    // Distinct heap pointers (so they exercise the by-value comparison path),
    // same numeric value.
    let Some(pa) = a.as_object_ptr() else {
        unreachable!("alloc_bigint returns an object value")
    };
    let Some(pb) = b.as_object_ptr() else {
        unreachable!("alloc_bigint returns an object value")
    };
    assert_ne!(pa, pb, "test setup must allocate distinct objects");
    assert!(
        equals(&mut vm, a, b),
        "two bigints with equal numeric value must compare equal regardless of Arc identity"
    );
}

#[test]
fn bigint_different_values() {
    let mut vm = make_vm();
    let a = alloc_bigint(&mut vm, BigInt::from(42));
    let b = alloc_bigint(&mut vm, BigInt::from(43));
    assert!(
        !equals(&mut vm, a, b),
        "bigints with different numeric values must not compare equal"
    );
}

#[test]
fn bigint_large_values() {
    let mut vm = make_vm();
    let big = BigInt::parse_bytes(b"99999999999999999999", 10).unwrap();
    let a = alloc_bigint(&mut vm, big.clone());
    let b = alloc_bigint(&mut vm, big);
    assert!(
        equals(&mut vm, a, b),
        "out-of-i64 bigints with equal value must compare equal"
    );
}
